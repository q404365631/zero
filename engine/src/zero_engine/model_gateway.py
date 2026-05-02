from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Protocol

from zero_engine.network import assert_public_profile_safe


MODEL_GATEWAY_STATUS_SCHEMA_VERSION = "zero.model_gateway.status.v1"
MODEL_GATEWAY_EVALUATION_SCHEMA_VERSION = "zero.model_gateway.evaluation.v1"

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


class ModelClient(Protocol):
    provider: str
    model: str
    capabilities: list[str]
    local: bool
    live_network: bool

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> dict[str, Any]:
        """Return a JSON-like dict or raise RuntimeError on unavailable providers."""


@dataclass(frozen=True)
class ModelGatewayConfig:
    provider: str = "none"
    model: str | None = None
    mock_enabled: bool = False
    allow_network: bool = False
    configured_providers: frozenset[str] = frozenset()

    def normalized_provider(self) -> str:
        provider = self.provider.strip().lower()
        return provider if provider in PROVIDER_CAPABILITIES else "none"


@dataclass
class ModelUsageEvent:
    provider: str
    model: str
    capability: str
    status: str
    prompt_chars: int
    output_chars: int
    estimated_cost_usd: float = 0.0

    def to_public_dict(self) -> dict[str, Any]:
        return {
            "provider": self.provider,
            "model": self.model,
            "capability": self.capability,
            "status": self.status,
            "prompt_chars": self.prompt_chars,
            "output_chars": self.output_chars,
            "estimated_cost_usd": round(self.estimated_cost_usd, 8),
        }


@dataclass
class ModelGateway:
    config: ModelGatewayConfig = field(default_factory=ModelGatewayConfig)
    usage_events: list[ModelUsageEvent] = field(default_factory=list)

    def clients(self) -> list[ModelClient]:
        clients: list[ModelClient] = [NullModelClient()]
        if self.config.mock_enabled or self.config.normalized_provider() == "mock":
            clients.append(MockModelClient(model=self.config.model or "zero-mock-v1"))
        for provider in ("openai", "anthropic", "ollama", "openrouter"):
            configured = provider in self.config.configured_providers
            clients.append(
                RegisteredExternalModelClient(
                    provider=provider,
                    model=self.config.model or f"{provider}:operator-configured",
                    configured=configured,
                    allow_network=self.config.allow_network,
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
        payload = {
            "schema_version": MODEL_GATEWAY_STATUS_SCHEMA_VERSION,
            "generated_at": generated_at,
            "mode": "fail_closed" if routing.get("structured_output") is None else "local_ready",
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
            output = client.generate_json(prompt, schema)
            validate_structured_output(output, schema)
        except (RuntimeError, ValueError) as exc:
            return self._failed_evaluation(
                capability=capability,
                prompt=prompt,
                reason=str(exc),
                provider=client.provider,
                model=client.model,
            )
        event = ModelUsageEvent(
            provider=client.provider,
            model=client.model,
            capability=capability,
            status="ok",
            prompt_chars=len(prompt),
            output_chars=len(json.dumps(output, sort_keys=True)),
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
    ) -> dict[str, Any]:
        event = ModelUsageEvent(
            provider=provider or "none",
            model=model or "none",
            capability=capability,
            status="failed_closed",
            prompt_chars=len(prompt),
            output_chars=0,
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


@dataclass(frozen=True)
class NullModelClient:
    provider: str = "none"
    model: str = "none"
    capabilities: list[str] = field(default_factory=list)
    local: bool = True
    live_network: bool = False

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> dict[str, Any]:
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

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> dict[str, Any]:
        del prompt
        output = {
            "verdict": "hold",
            "confidence": 0.0,
            "rationale": "mock provider returns deterministic advisory hold",
        }
        for key in schema.get("required", []):
            output.setdefault(key, _default_value_for_type(schema.get("properties", {}).get(key, {})))
        return output


@dataclass(frozen=True)
class RegisteredExternalModelClient:
    provider: str
    model: str
    configured: bool
    allow_network: bool = False
    local: bool = False
    live_network: bool = True

    @property
    def capabilities(self) -> list[str]:
        return PROVIDER_CAPABILITIES[self.provider]

    def generate_json(self, prompt: str, schema: dict[str, Any]) -> dict[str, Any]:
        del prompt, schema
        if not self.configured:
            raise RuntimeError(f"{self.provider} is not configured")
        if not self.allow_network:
            raise RuntimeError(f"{self.provider} network calls are disabled")
        raise RuntimeError(f"{self.provider} adapter is registered but not enabled in the open runtime")


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
