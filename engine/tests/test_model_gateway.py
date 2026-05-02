from __future__ import annotations

import json
from datetime import UTC, datetime

import pytest
from zero_engine.api import PaperApi, PaperApiState
from zero_engine.model_gateway import (
    ModelGateway,
    ModelGatewayConfig,
    validate_structured_output,
)


FIXED_DT = datetime(2026, 5, 1, tzinfo=UTC)

ADVISORY_SCHEMA = {
    "type": "object",
    "required": ["verdict", "confidence", "rationale"],
    "properties": {
        "verdict": {"type": "string"},
        "confidence": {"type": "number"},
        "rationale": {"type": "string"},
    },
}


class StubJsonTransport:
    def __init__(self, response: dict[str, object] | RuntimeError) -> None:
        self.response = response
        self.requests: list[dict[str, object]] = []

    def post_json(
        self,
        *,
        url: str,
        headers: dict[str, str],
        payload: dict[str, object],
        timeout_s: float,
    ) -> dict[str, object]:
        self.requests.append(
            {
                "url": url,
                "headers": dict(headers),
                "payload": payload,
                "timeout_s": timeout_s,
            }
        )
        if isinstance(self.response, RuntimeError):
            raise self.response
        return self.response


def test_model_gateway_status_is_fail_closed_without_provider() -> None:
    gateway = ModelGateway()

    status = gateway.status(generated_at="2026-05-01T00:00:00+00:00")

    assert status["schema_version"] == "zero.model_gateway.status.v1"
    assert status["mode"] == "fail_closed"
    assert status["routing"]["structured_output"] is None
    assert status["usage"]["events"] == 0
    body = json.dumps(status)
    assert "sk-" not in body
    assert "private_key" not in body


def test_model_gateway_mock_provider_evaluates_structured_output() -> None:
    gateway = ModelGateway(ModelGatewayConfig(provider="mock", mock_enabled=True))

    result = gateway.evaluate_json(
        capability="structured_output",
        prompt="Public aggregate only; no raw trades.",
        schema=ADVISORY_SCHEMA,
    )
    status = gateway.status(generated_at="2026-05-01T00:00:00+00:00")

    assert result["schema_version"] == "zero.model_gateway.evaluation.v1"
    assert result["status"] == "ok"
    assert result["provider"] == "mock"
    assert result["output"]["verdict"] == "hold"
    assert result["safety"]["trading_dependency"] == "advisory_only"
    assert status["mode"] == "local_ready"
    assert status["usage"]["events"] == 1
    assert status["usage"]["recent"][0]["prompt_chars"] > 0
    assert "Public aggregate" not in json.dumps(status)


def test_model_gateway_fails_closed_on_unavailable_external_provider() -> None:
    gateway = ModelGateway(
        ModelGatewayConfig(
            provider="openai",
            configured_providers=frozenset({"openai"}),
            allow_network=False,
        )
    )

    result = gateway.evaluate_json(
        capability="structured_output",
        prompt="advisory",
        schema=ADVISORY_SCHEMA,
    )

    assert result["status"] == "failed_closed"
    assert result["provider"] == "openai"
    assert result["confidence"] == 0.0
    assert result["output"] is None
    assert result["safety"]["fail_closed"] is True
    assert "disabled" in result["reason"]


@pytest.mark.parametrize(
    ("provider", "response"),
    [
        (
            "openai",
            {"output_text": '{"verdict":"hold","confidence":0.7,"rationale":"ok"}'},
        ),
        (
            "anthropic",
            {
                "content": [
                    {"type": "text", "text": '{"verdict":"hold","confidence":0.6,"rationale":"ok"}'}
                ]
            },
        ),
        (
            "ollama",
            {"response": '{"verdict":"hold","confidence":0.5,"rationale":"ok"}'},
        ),
        (
            "openrouter",
            {
                "choices": [
                    {"message": {"content": '{"verdict":"hold","confidence":0.8,"rationale":"ok"}'}}
                ]
            },
        ),
    ],
)
def test_external_provider_adapters_parse_structured_json(
    provider: str,
    response: dict[str, object],
) -> None:
    transport = StubJsonTransport(response)
    gateway = ModelGateway(
        ModelGatewayConfig(
            provider=provider,
            model=f"{provider}-test-model",
            allow_network=True,
            configured_providers=frozenset({provider}),
            provider_credentials={
                "openai": "test-openai-key",
                "anthropic": "test-anthropic-key",
                "openrouter": "test-openrouter-key",
            },
            transport=transport,
        )
    )

    result = gateway.evaluate_json(
        capability="structured_output",
        prompt="Use public aggregates only.",
        schema=ADVISORY_SCHEMA,
    )
    status = gateway.status(generated_at="2026-05-01T00:00:00+00:00")

    assert result["status"] == "ok"
    assert result["provider"] == provider
    assert result["output"]["verdict"] == "hold"
    assert status["mode"] == "external_ready"
    assert status["routing"]["structured_output"] == provider
    assert status["privacy"]["contains_api_keys"] is False
    body = json.dumps(status)
    assert "test-openai-key" not in body
    assert "test-anthropic-key" not in body
    assert "test-openrouter-key" not in body
    assert transport.requests
    payload = transport.requests[0]["payload"]
    assert isinstance(payload, dict)
    if provider == "openai":
        text = payload["text"]
        assert isinstance(text, dict)
        assert text["format"]["type"] == "json_schema"
    elif provider == "ollama":
        assert payload["format"] == ADVISORY_SCHEMA
    elif provider == "openrouter":
        assert payload["response_format"]["type"] == "json_schema"


def test_external_provider_transport_errors_fail_closed_without_secret_leak() -> None:
    transport = StubJsonTransport(RuntimeError("socket failure with key metadata"))
    gateway = ModelGateway(
        ModelGatewayConfig(
            provider="openai",
            model="openai-test-model",
            allow_network=True,
            configured_providers=frozenset({"openai"}),
            provider_credentials={"openai": "sk-test-secret"},
            transport=transport,
        )
    )

    result = gateway.evaluate_json(
        capability="structured_output",
        prompt="advisory",
        schema=ADVISORY_SCHEMA,
    )
    status = gateway.status(generated_at="2026-05-01T00:00:00+00:00")

    assert result["status"] == "failed_closed"
    assert result["provider"] == "openai"
    assert result["output"] is None
    assert result["reason"] == "openai request failed"
    assert "sk-test-secret" not in json.dumps(result)
    assert "sk-test-secret" not in json.dumps(status)


def test_structured_output_validation_rejects_missing_or_wrong_types() -> None:
    validate_structured_output(
        {"verdict": "hold", "confidence": 0.0, "rationale": "ok"},
        ADVISORY_SCHEMA,
    )
    with pytest.raises(ValueError, match="missing required key"):
        validate_structured_output({"verdict": "hold"}, ADVISORY_SCHEMA)
    with pytest.raises(ValueError, match="wrong type"):
        validate_structured_output(
            {"verdict": "hold", "confidence": "high", "rationale": "bad"},
            ADVISORY_SCHEMA,
        )


def test_paper_api_exposes_public_model_gateway_status() -> None:
    api = PaperApi(
        PaperApiState(
            clock=lambda: FIXED_DT,
            started_at=FIXED_DT,
            model_gateway_provider="mock",
            model_gateway_mock_enabled=True,
        )
    )

    status, payload = api.get("/intelligence/model-gateway", {})

    assert status == 200
    assert payload["schema_version"] == "zero.model_gateway.status.v1"
    assert payload["mode"] == "local_ready"
    assert payload["routing"]["structured_output"] == "mock"
    assert payload["privacy"]["prompts_included"] is False
