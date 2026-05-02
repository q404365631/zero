from __future__ import annotations

import json
import urllib.error
import urllib.request
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any, Protocol

from zero_engine.network import assert_public_profile_safe


MODEL_GATEWAY_STATUS_SCHEMA_VERSION = "zero.model_gateway.status.v1"
MODEL_GATEWAY_EVALUATION_SCHEMA_VERSION = "zero.model_gateway.evaluation.v1"
MODEL_GATEWAY_HEALTH_SCHEMA_VERSION = "zero.model_gateway.health.v1"
MODEL_GATEWAY_AUDIT_SCHEMA_VERSION = "zero.model_gateway.audit.v1"

CAPABILITIES = {
    "hard_reasoning",
    "fast_reasoning",
    "chat",
    "embeddings",
    "structured_output",
}

PROVIDER_CAPABILITIES: dict[str, list[str]] = {
    "none": [],
    "mock": ["hard_reasoning", "fast_reasoning", "chat", "structured_output"],
    "openai": ["hard_reasoning", "fast_reasoning", "chat", "embeddings", "structured_output"],
    "anthropic": ["hard_reasoning", "fast_reasoning", "chat", "structured_output"],
    "ollama": ["fast_reasoning", "chat", "embeddings", "structured_output"],
    "openrouter": ["hard_reasoning", "fast_reasoning", "chat", "structured_output"],
}

PROVIDER_REQUIRED_ENV: dict[str, list[str]] = {
    "openai": ["OPENAI_API_KEY"],
    "anthropic": ["ANTHROPIC_API_KEY"],
    "ollama": ["OLLAMA_BASE_URL"],
    "openrouter": ["OPENROUTER_API_KEY"],
}

PROVIDER_DEFAULT_ENDPOINTS: dict[str, str] = {
    "openai": "https://api.openai.com/v1/responses",
    "anthropic": "https://api.anthropic.com/v1/messages",
    "ollama": "http://127.0.0.1:11434/api/generate",
    "openrouter": "https://openrouter.ai/api/v1/chat/completions",
}


class ModelClient(Protocol):
    provider: str
    model: str
    capabilities: list[str]
    local: bool
    live_network: bool

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> ModelGeneration:
        """Return structured model output or raise RuntimeError on unavailable providers."""


class JsonHttpTransport(Protocol):
    def post_json(
        self,
        *,
        url: str,
        headers: Mapping[str, str],
        payload: dict[str, Any],
        timeout_s: float,
    ) -> dict[str, Any]:
        """Post JSON and return the decoded JSON response."""


@dataclass(frozen=True)
class ModelGatewayConfig:
    provider: str = "none"
    model: str | None = None
    mock_enabled: bool = False
    allow_network: bool = False
    configured_providers: frozenset[str] = frozenset()
    provider_credentials: Mapping[str, str] = field(default_factory=dict, repr=False)
    provider_endpoints: Mapping[str, str] = field(default_factory=dict)
    provider_input_cost_per_1m_tokens_usd: Mapping[str, float] = field(default_factory=dict)
    provider_output_cost_per_1m_tokens_usd: Mapping[str, float] = field(default_factory=dict)
    max_attempts: int = 1
    timeout_s: float = 30.0
    transport: JsonHttpTransport | None = field(default=None, repr=False, compare=False)

    def normalized_provider(self) -> str:
        provider = self.provider.strip().lower()
        return provider if provider in PROVIDER_CAPABILITIES else "none"

    def normalized_max_attempts(self) -> int:
        return min(3, max(1, int(self.max_attempts)))

    def normalized_timeout_s(self) -> float:
        return min(120.0, max(1.0, float(self.timeout_s)))


@dataclass
class ModelUsageEvent:
    provider: str
    model: str
    capability: str
    status: str
    prompt_chars: int
    output_chars: int
    attempts: int = 0
    input_tokens: int | None = None
    output_tokens: int | None = None
    estimated_cost_usd: float = 0.0
    cost_estimate_source: str = "unpriced"

    def to_public_dict(self) -> dict[str, Any]:
        return {
            "provider": self.provider,
            "model": self.model,
            "capability": self.capability,
            "status": self.status,
            "prompt_chars": self.prompt_chars,
            "output_chars": self.output_chars,
            "attempts": self.attempts,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "estimated_cost_usd": round(self.estimated_cost_usd, 8),
            "cost_estimate_source": self.cost_estimate_source,
        }


@dataclass(frozen=True)
class ModelGeneration:
    output: dict[str, Any]
    attempts: int = 1
    input_tokens: int | None = None
    output_tokens: int | None = None


@dataclass
class ModelGateway:
    config: ModelGatewayConfig = field(default_factory=ModelGatewayConfig)
    usage_events: list[ModelUsageEvent] = field(default_factory=list)

    def clients(self) -> list[ModelClient]:
        clients: list[ModelClient] = [NullModelClient()]
        if self.config.mock_enabled or self.config.normalized_provider() == "mock":
            clients.append(MockModelClient(model=self.config.model or "zero-mock-v1"))
        transport = self.config.transport or UrllibJsonTransport()
        for provider in ("openai", "anthropic", "ollama", "openrouter"):
            configured = provider in self.config.configured_providers
            clients.append(
                HttpJsonExternalModelClient(
                    provider=provider,
                    model=self.config.model or f"{provider}:operator-configured",
                    configured=configured,
                    allow_network=self.config.allow_network,
                    credential=self.config.provider_credentials.get(provider),
                    endpoint=self.config.provider_endpoints.get(
                        provider,
                        PROVIDER_DEFAULT_ENDPOINTS[provider],
                    ),
                    transport=transport,
                    max_attempts=self.config.normalized_max_attempts(),
                    timeout_s=self.config.normalized_timeout_s(),
                )
            )
        return clients

    def status(self, *, generated_at: str) -> dict[str, Any]:
        providers = [provider_status(client, self.config) for client in self.clients()]
        routing = {
            capability: self._selected_provider(capability)
            for capability in sorted(CAPABILITIES)
            if capability != "embeddings"
        }
        selected = routing.get("structured_output")
        mode = "fail_closed"
        if selected == "mock":
            mode = "local_ready"
        elif selected in PROVIDER_DEFAULT_ENDPOINTS:
            mode = "external_ready"
        payload = {
            "schema_version": MODEL_GATEWAY_STATUS_SCHEMA_VERSION,
            "generated_at": generated_at,
            "mode": mode,
            "default_provider": self.config.normalized_provider(),
            "routing": routing,
            "providers": providers,
            "usage": {
                "events": len(self.usage_events),
                "total_estimated_cost_usd": round(
                    sum(event.estimated_cost_usd for event in self.usage_events),
                    8,
                ),
                "recent": [event.to_public_dict() for event in self.usage_events[-10:]],
                "cost_policy": {
                    "pricing_source": "operator_configured"
                    if self.config.provider_input_cost_per_1m_tokens_usd
                    or self.config.provider_output_cost_per_1m_tokens_usd
                    else "unpriced",
                    "price_values_included": False,
                },
            },
            "privacy": {
                "prompts_included": False,
                "raw_model_outputs_included": False,
                "contains_api_keys": False,
                "contains_exchange_credentials": False,
                "contains_wallet_material": False,
            },
        }
        assert_model_gateway_safe(payload)
        return payload

    def health(self, *, generated_at: str, run_network_probe: bool = False) -> dict[str, Any]:
        selected = self._select_client("structured_output")
        checks = [
            provider_health_probe(
                client,
                self.config,
                selected_provider=None if selected is None else selected.provider,
            )
            for client in self.clients()
        ]
        network_probe = {
            "requested": run_network_probe,
            "performed": False,
            "status": "not_requested",
            "provider": None,
            "attempts": 0,
            "input_tokens": None,
            "output_tokens": None,
            "reason": "set network=true to run an explicit provider probe",
        }
        if run_network_probe:
            network_probe = self._network_health_probe(selected)

        selected_check = next((check for check in checks if check["selected"]), None)
        if selected_check is None:
            summary_status = "failed_closed"
        elif network_probe["performed"]:
            summary_status = network_probe["status"]
        elif selected_check["status"] in {"local_ready", "ready_for_probe"}:
            summary_status = "ready"
        else:
            summary_status = "degraded"

        payload = {
            "schema_version": MODEL_GATEWAY_HEALTH_SCHEMA_VERSION,
            "generated_at": generated_at,
            "status": summary_status,
            "selected_provider": None if selected is None else selected.provider,
            "network_probe": network_probe,
            "checks": checks,
            "safety": {
                "fail_closed": summary_status not in {"ready", "healthy"},
                "advisory_only": True,
                "network_probe_requires_explicit_query": True,
                "prompts_included": False,
                "raw_model_outputs_included": False,
            },
        }
        assert_model_gateway_safe(payload)
        return payload

    def audit_bundle(self, *, generated_at: str) -> dict[str, Any]:
        status = self.status(generated_at=generated_at)
        health = self.health(generated_at=generated_at, run_network_probe=False)
        usage = status["usage"]
        controls = {
            "fail_closed_default": True,
            "advisory_only": True,
            "structured_output_validation": True,
            "bounded_retry_policy": True,
            "network_disabled_by_default": True,
            "operator_configured_pricing_only": True,
            "prompts_persisted": False,
            "raw_outputs_persisted": False,
            "provider_request_ids_persisted": False,
        }
        payload = {
            "schema_version": MODEL_GATEWAY_AUDIT_SCHEMA_VERSION,
            "generated_at": generated_at,
            "status": {
                "mode": status["mode"],
                "default_provider": status["default_provider"],
                "selected_provider": health["selected_provider"],
            },
            "health": health,
            "usage": {
                "events": usage["events"],
                "total_estimated_cost_usd": usage["total_estimated_cost_usd"],
                "cost_policy": usage["cost_policy"],
            },
            "controls": controls,
            "evidence_requirements": [
                "status packet captured before production enablement",
                "health packet captured with network=false",
                "explicit network=true probe captured only in a controlled canary",
                "provider usage counters reviewed without prompts or raw outputs",
                "operator confirms model output remains advisory-only",
            ],
            "privacy": status["privacy"]
            | {
                "provider_request_ids_included": False,
                "headers_included": False,
            },
        }
        assert_model_gateway_safe(payload)
        return payload

    def evaluate_json(
        self,
        *,
        capability: str,
        prompt: str,
        schema: dict[str, Any],
    ) -> dict[str, Any]:
        if capability not in CAPABILITIES:
            return self._failed_evaluation(
                capability=capability,
                prompt=prompt,
                reason="unsupported capability",
            )
        client = self._select_client(capability)
        if client is None:
            return self._failed_evaluation(
                capability=capability,
                prompt=prompt,
                reason="no configured model provider for capability",
            )
        try:
            generation = client.generate_json(prompt, schema)
            output = generation.output
            validate_structured_output(output, schema)
        except (RuntimeError, ValueError) as exc:
            return self._failed_evaluation(
                capability=capability,
                prompt=prompt,
                reason=str(exc),
                provider=client.provider,
                model=client.model,
                attempts=getattr(client, "max_attempts", 1),
            )
        event = ModelUsageEvent(
            provider=client.provider,
            model=client.model,
            capability=capability,
            status="ok",
            prompt_chars=len(prompt),
            output_chars=len(json.dumps(output, sort_keys=True)),
            attempts=generation.attempts,
            input_tokens=generation.input_tokens,
            output_tokens=generation.output_tokens,
            estimated_cost_usd=self._estimate_cost_usd(
                provider=client.provider,
                input_tokens=generation.input_tokens,
                output_tokens=generation.output_tokens,
            ),
            cost_estimate_source=self._cost_estimate_source(client.provider, generation),
        )
        self.usage_events.append(event)
        result = {
            "schema_version": MODEL_GATEWAY_EVALUATION_SCHEMA_VERSION,
            "status": "ok",
            "provider": client.provider,
            "model": client.model,
            "capability": capability,
            "confidence": float(output.get("confidence", 0.0)),
            "output": output,
            "usage": event.to_public_dict(),
            "safety": {
                "fail_closed": False,
                "trading_dependency": "advisory_only",
            },
        }
        assert_model_gateway_safe(result)
        return result

    def _select_client(self, capability: str) -> ModelClient | None:
        preferred = self.config.normalized_provider()
        for client in self.clients():
            if (
                client.provider == preferred
                and capability in client.capabilities
                and client.provider != "none"
            ):
                return client
        for client in self.clients():
            if capability in client.capabilities and client.provider == "mock":
                return client
        return None

    def _selected_provider(self, capability: str) -> str | None:
        client = self._select_client(capability)
        return None if client is None else client.provider

    def _failed_evaluation(
        self,
        *,
        capability: str,
        prompt: str,
        reason: str,
        provider: str | None = None,
        model: str | None = None,
        attempts: int | None = None,
    ) -> dict[str, Any]:
        event = ModelUsageEvent(
            provider=provider or "none",
            model=model or "none",
            capability=capability,
            status="failed_closed",
            prompt_chars=len(prompt),
            output_chars=0,
            attempts=attempts if attempts is not None else (1 if provider else 0),
        )
        self.usage_events.append(event)
        result = {
            "schema_version": MODEL_GATEWAY_EVALUATION_SCHEMA_VERSION,
            "status": "failed_closed",
            "provider": provider,
            "model": model,
            "capability": capability,
            "confidence": 0.0,
            "output": None,
            "reason": reason,
            "usage": event.to_public_dict(),
            "safety": {
                "fail_closed": True,
                "trading_dependency": "advisory_only",
            },
        }
        assert_model_gateway_safe(result)
        return result

    def _estimate_cost_usd(
        self,
        *,
        provider: str,
        input_tokens: int | None,
        output_tokens: int | None,
    ) -> float:
        if input_tokens is None and output_tokens is None:
            return 0.0
        input_rate = self.config.provider_input_cost_per_1m_tokens_usd.get(provider)
        output_rate = self.config.provider_output_cost_per_1m_tokens_usd.get(provider)
        if input_rate is None and output_rate is None:
            return 0.0
        input_cost = (input_tokens or 0) * (input_rate or 0.0) / 1_000_000
        output_cost = (output_tokens or 0) * (output_rate or 0.0) / 1_000_000
        return input_cost + output_cost

    def _cost_estimate_source(self, provider: str, generation: ModelGeneration) -> str:
        if generation.input_tokens is None and generation.output_tokens is None:
            return "no_provider_usage"
        if (
            provider in self.config.provider_input_cost_per_1m_tokens_usd
            or provider in self.config.provider_output_cost_per_1m_tokens_usd
        ):
            return "operator_configured_token_price"
        return "usage_only_unpriced"

    def _network_health_probe(self, selected: ModelClient | None) -> dict[str, Any]:
        if selected is None:
            return {
                "requested": True,
                "performed": False,
                "status": "failed_closed",
                "provider": None,
                "attempts": 0,
                "input_tokens": None,
                "output_tokens": None,
                "reason": "no configured model provider for structured_output",
            }
        schema = {
            "type": "object",
            "required": ["ok", "confidence", "note"],
            "properties": {
                "ok": {"type": "boolean"},
                "confidence": {"type": "number"},
                "note": {"type": "string"},
            },
        }
        try:
            generation = selected.generate_json(
                "ZERO model gateway health probe. Return only public readiness JSON.",
                schema,
            )
            validate_structured_output(generation.output, schema)
        except (RuntimeError, ValueError) as exc:
            return {
                "requested": True,
                "performed": False,
                "status": "failed_closed",
                "provider": selected.provider,
                "attempts": getattr(selected, "max_attempts", 1),
                "input_tokens": None,
                "output_tokens": None,
                "reason": str(exc),
            }
        return {
            "requested": True,
            "performed": True,
            "status": "healthy",
            "provider": selected.provider,
            "attempts": generation.attempts,
            "input_tokens": generation.input_tokens,
            "output_tokens": generation.output_tokens,
            "reason": "structured health probe returned valid public JSON",
        }


@dataclass(frozen=True)
class NullModelClient:
    provider: str = "none"
    model: str = "none"
    capabilities: list[str] = field(default_factory=list)
    local: bool = True
    live_network: bool = False

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> ModelGeneration:
        raise RuntimeError("no model provider configured")


@dataclass(frozen=True)
class MockModelClient:
    model: str = "zero-mock-v1"
    provider: str = "mock"
    capabilities: list[str] = field(
        default_factory=lambda: ["hard_reasoning", "fast_reasoning", "chat", "structured_output"]
    )
    local: bool = True
    live_network: bool = False

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> ModelGeneration:
        del prompt
        output = {
            "verdict": "hold",
            "confidence": 0.0,
            "rationale": "mock provider returns deterministic advisory hold",
        }
        for key in schema.get("required", []):
            output.setdefault(
                key, _default_value_for_type(schema.get("properties", {}).get(key, {}))
            )
        return ModelGeneration(output=output, attempts=1)


@dataclass(frozen=True)
class UrllibJsonTransport:
    def post_json(
        self,
        *,
        url: str,
        headers: Mapping[str, str],
        payload: dict[str, Any],
        timeout_s: float,
    ) -> dict[str, Any]:
        request = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            headers={"content-type": "application/json", **dict(headers)},
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout_s) as response:
                body = response.read().decode("utf-8")
        except urllib.error.URLError as exc:
            raise RuntimeError("model provider request failed") from exc
        try:
            decoded = json.loads(body)
        except json.JSONDecodeError as exc:
            raise RuntimeError("model provider returned invalid JSON") from exc
        if not isinstance(decoded, dict):
            raise RuntimeError("model provider returned a non-object JSON response")
        return decoded


@dataclass(frozen=True)
class HttpJsonExternalModelClient:
    provider: str
    model: str
    configured: bool
    allow_network: bool = False
    credential: str | None = field(default=None, repr=False)
    endpoint: str = ""
    transport: JsonHttpTransport = field(default_factory=UrllibJsonTransport, repr=False)
    local: bool = False
    live_network: bool = True
    timeout_s: float = 30.0
    max_attempts: int = 1

    @property
    def capabilities(self) -> list[str]:
        return PROVIDER_CAPABILITIES[self.provider]

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> ModelGeneration:
        if not self.configured:
            raise RuntimeError(f"{self.provider} is not configured")
        if not self.allow_network:
            raise RuntimeError(f"{self.provider} network calls are disabled")
        if self.model.endswith(":operator-configured"):
            raise RuntimeError(f"{self.provider} model name is not configured")
        request_prompt = _structured_output_prompt(prompt, schema)
        last_error: RuntimeError | None = None
        attempts = min(3, max(1, self.max_attempts))
        for attempt in range(1, attempts + 1):
            try:
                response = self.transport.post_json(
                    url=self.endpoint,
                    headers=self._headers(),
                    payload=self._payload(request_prompt, schema),
                    timeout_s=self.timeout_s,
                )
                return ModelGeneration(
                    output=self._parse_response(response),
                    attempts=attempt,
                    input_tokens=_extract_input_tokens(response),
                    output_tokens=_extract_output_tokens(response),
                )
            except RuntimeError as exc:
                last_error = exc
        raise RuntimeError(f"{self.provider} request failed") from last_error

    def _headers(self) -> dict[str, str]:
        if self.provider == "openai":
            return {"authorization": f"Bearer {self._required_credential()}"}
        if self.provider == "anthropic":
            return {
                "x-api-key": self._required_credential(),
                "anthropic-version": "2023-06-01",
            }
        if self.provider == "openrouter":
            return {"authorization": f"Bearer {self._required_credential()}"}
        return {}

    def _payload(self, prompt: str, schema: dict[str, Any]) -> dict[str, Any]:
        if self.provider == "openai":
            return {
                "model": self.model,
                "input": prompt,
                "text": {
                    "format": {
                        "type": "json_schema",
                        "name": "zero_model_gateway_output",
                        "schema": schema,
                        "strict": False,
                    }
                },
            }
        if self.provider == "anthropic":
            return {
                "model": self.model,
                "max_tokens": 1024,
                "messages": [{"role": "user", "content": prompt}],
            }
        if self.provider == "ollama":
            return {
                "model": self.model,
                "prompt": prompt,
                "format": schema or "json",
                "stream": False,
            }
        if self.provider == "openrouter":
            return {
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "zero_model_gateway_output",
                        "schema": schema,
                    },
                },
            }
        raise RuntimeError(f"{self.provider} provider is unsupported")

    def _parse_response(self, response: dict[str, Any]) -> dict[str, Any]:
        if self.provider == "openai":
            return _extract_openai_json(response)
        if self.provider == "anthropic":
            return _extract_text_content_json(response)
        if self.provider == "ollama":
            candidate = response.get("response")
            if isinstance(candidate, dict):
                return candidate
            if isinstance(candidate, str):
                return _decode_json_object(candidate, "ollama response")
        if self.provider == "openrouter":
            choices = response.get("choices")
            if isinstance(choices, list) and choices:
                message = choices[0].get("message") if isinstance(choices[0], dict) else None
                if isinstance(message, dict) and isinstance(message.get("content"), str):
                    return _decode_json_object(message["content"], "openrouter message")
        raise RuntimeError(f"{self.provider} response did not include a JSON object")

    def _required_credential(self) -> str:
        if not self.credential:
            raise RuntimeError(f"{self.provider} credential is not configured")
        return self.credential


def provider_status(client: ModelClient, config: ModelGatewayConfig) -> dict[str, Any]:
    provider = client.provider
    configured = provider == "mock" and config.mock_enabled
    if provider in config.configured_providers:
        configured = True
    if provider == "none":
        configured = False
    return {
        "provider": provider,
        "model": client.model,
        "configured": configured,
        "local": client.local,
        "live_network": client.live_network,
        "network_allowed": bool(config.allow_network and configured and client.live_network),
        "capabilities": list(client.capabilities),
        "required_env": PROVIDER_REQUIRED_ENV.get(provider, []),
        "adapter": "http_json" if provider in PROVIDER_DEFAULT_ENDPOINTS else "local",
        "retry_policy": {
            "max_attempts": config.normalized_max_attempts() if client.live_network else 0,
            "timeout_s": config.normalized_timeout_s() if client.live_network else 0,
        },
    }


def provider_health_probe(
    client: ModelClient,
    config: ModelGatewayConfig,
    *,
    selected_provider: str | None,
) -> dict[str, Any]:
    provider = client.provider
    status = provider_status(client, config)
    reasons: list[str] = []
    probe_status = "unavailable"
    if provider == "none":
        reasons.append("null provider is the fail-closed fallback")
    elif provider == "mock":
        if config.mock_enabled or config.normalized_provider() == "mock":
            probe_status = "local_ready"
            reasons.append("deterministic local provider is enabled")
        else:
            reasons.append("mock provider is not enabled")
    elif provider in PROVIDER_DEFAULT_ENDPOINTS:
        if not status["configured"]:
            reasons.append("required provider environment is missing")
        elif not config.allow_network:
            probe_status = "network_disabled"
            reasons.append("network boundary is configured but disabled")
        elif client.model.endswith(":operator-configured"):
            probe_status = "misconfigured"
            reasons.append("model name is not configured")
        else:
            probe_status = "ready_for_probe"
            reasons.append("provider can be probed with explicit network=true")
    return {
        "provider": provider,
        "status": probe_status,
        "selected": provider == selected_provider,
        "configured": status["configured"],
        "network_allowed": status["network_allowed"],
        "credential_configured": bool(provider in config.configured_providers),
        "model_configured": not client.model.endswith(":operator-configured"),
        "capabilities": status["capabilities"],
        "retry_policy": status["retry_policy"],
        "reasons": reasons,
    }


def validate_structured_output(output: dict[str, Any], schema: dict[str, Any]) -> None:
    if not isinstance(output, dict):
        raise ValueError("model output must be a JSON object")
    properties = schema.get("properties", {})
    for key in schema.get("required", []):
        if key not in output:
            raise ValueError(f"model output missing required key: {key}")
        expected = properties.get(key, {}).get("type")
        if expected and not _matches_json_type(output[key], expected):
            raise ValueError(f"model output key {key} has wrong type")


def assert_model_gateway_safe(payload: dict[str, Any]) -> None:
    assert_public_profile_safe(payload)
    body = json.dumps(payload, sort_keys=True).lower()
    for token in ("sk-", "private_key", "trace-", "idempotency"):
        if token in body:
            raise ValueError(f"model gateway packet contains forbidden token: {token}")


def _matches_json_type(value: Any, expected: str | list[str]) -> bool:
    expected_types = expected if isinstance(expected, list) else [expected]
    for item in expected_types:
        if item == "string" and isinstance(value, str):
            return True
        if item == "number" and isinstance(value, int | float) and not isinstance(value, bool):
            return True
        if item == "integer" and isinstance(value, int) and not isinstance(value, bool):
            return True
        if item == "boolean" and isinstance(value, bool):
            return True
        if item == "object" and isinstance(value, dict):
            return True
        if item == "array" and isinstance(value, list):
            return True
        if item == "null" and value is None:
            return True
    return False


def _structured_output_prompt(prompt: str, schema: dict[str, Any]) -> str:
    return (
        f"{prompt}\n\n"
        "Return only a JSON object that matches this JSON schema:\n"
        f"{json.dumps(schema, sort_keys=True)}"
    )


def _extract_openai_json(response: dict[str, Any]) -> dict[str, Any]:
    output_json = response.get("output_json")
    if isinstance(output_json, dict):
        return output_json
    output_text = response.get("output_text")
    if isinstance(output_text, str):
        return _decode_json_object(output_text, "openai output_text")
    output = response.get("output")
    if isinstance(output, list):
        for item in output:
            if not isinstance(item, dict):
                continue
            content = item.get("content")
            if not isinstance(content, list):
                continue
            for part in content:
                if isinstance(part, dict) and isinstance(part.get("text"), str):
                    return _decode_json_object(part["text"], "openai content text")
    raise RuntimeError("openai response did not include a JSON object")


def _extract_text_content_json(response: dict[str, Any]) -> dict[str, Any]:
    content = response.get("content")
    if not isinstance(content, list):
        raise RuntimeError("anthropic response did not include content")
    for part in content:
        if isinstance(part, dict) and isinstance(part.get("text"), str):
            return _decode_json_object(part["text"], "anthropic content text")
    raise RuntimeError("anthropic response did not include text content")


def _decode_json_object(raw: str, label: str) -> dict[str, Any]:
    try:
        decoded = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"{label} was not valid JSON") from exc
    if not isinstance(decoded, dict):
        raise RuntimeError(f"{label} was not a JSON object")
    return decoded


def _extract_input_tokens(response: dict[str, Any]) -> int | None:
    usage = response.get("usage")
    if not isinstance(usage, dict):
        return None
    for key in ("input_tokens", "prompt_tokens"):
        value = usage.get(key)
        if isinstance(value, int) and value >= 0:
            return value
    return None


def _extract_output_tokens(response: dict[str, Any]) -> int | None:
    usage = response.get("usage")
    if not isinstance(usage, dict):
        return None
    for key in ("output_tokens", "completion_tokens"):
        value = usage.get(key)
        if isinstance(value, int) and value >= 0:
            return value
    return None


def _default_value_for_type(property_schema: dict[str, Any]) -> Any:
    expected = property_schema.get("type", "string")
    first = expected[0] if isinstance(expected, list) else expected
    if first == "number":
        return 0.0
    if first == "integer":
        return 0
    if first == "boolean":
        return False
    if first == "object":
        return {}
    if first == "array":
        return []
    return ""
