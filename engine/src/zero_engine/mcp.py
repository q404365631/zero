from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Callable
from pathlib import Path
from typing import Any, TextIO

from zero_engine import PaperEngine, load_scenario, load_strategy_runner, parse_scenario

SERVER_NAME = "zero-mcp"
SERVER_VERSION = "0.1.1"
DEFAULT_PROTOCOL_VERSION = "2025-06-18"
SUPPORTED_PROTOCOL_VERSIONS = {"2025-06-18", "2025-11-25"}
PAPER_TS = 1777646400.0

JsonMap = dict[str, Any]

EMBEDDED_SCENARIO: JsonMap = {
    "name": "paper-launch-smoke",
    "mode": "paper",
    "limits": {
        "max_notional_usd": 500,
        "max_position_notional_usd": 900,
        "min_confidence": 0.7,
    },
    "orders": [
        {"symbol": "BTC", "side": "buy", "quantity": 0.01, "price": 40000, "confidence": 0.84},
        {"symbol": "ETH", "side": "buy", "quantity": 1, "price": 3000, "confidence": 0.93},
        {
            "symbol": "BTC",
            "side": "sell",
            "quantity": 0.005,
            "price": 40500,
            "confidence": 0.1,
            "reduce_only": True,
        },
        {"symbol": "SOL", "side": "buy", "quantity": 10, "price": 140, "confidence": 0.95},
    ],
}

EMBEDDED_MARKET: JsonMap = {
    "BTC": {"as_of": "2026-05-01T00:05:00Z", "last": 40500.0},
    "ETH": {"as_of": "2026-05-01T00:00:00Z", "last": 3000.0},
    "SOL": {"as_of": "2026-05-01T00:00:00Z", "last": 140.0},
}

EMBEDDED_PROOF_PACK: JsonMap = {
    "schema_version": "zero.proof_pack.v1",
    "generated_at": "2026-05-01T00:00:00Z",
    "name": "demo-paper-launch-smoke",
    "mode": "paper",
    "claim_boundary": {
        "paper_mode_verified": True,
        "live_trading_claimed": False,
        "paper_vs_live_correlation_claimed": False,
        "pnl_claimed": False,
    },
    "paper": {
        "scenario": "paper-launch-smoke",
        "decisions": 4,
        "fills": 2,
        "rejections": 2,
        "open_positions": 1,
        "symbols": ["BTC", "ETH", "SOL"],
    },
    "live_correlation": {
        "status": "unavailable",
        "reason": "requires signed paper/live records and exchange-side evidence",
        "r_squared": None,
    },
    "privacy": {
        "contains_exchange_credentials": False,
        "contains_wallet_material": False,
        "contains_raw_exchange_order_ids": False,
        "contains_private_notes": False,
    },
    "artifacts": {
        "scenario_json": "embedded",
        "candles_jsonl": "embedded",
        "paper_decisions_csv": "source-checkout-only",
        "paper_proof_svg": "source-checkout-only",
        "demo_readme": "source-checkout-only",
    },
    "proof_hash": "embedded-source-checkout-required-for-file-hash-verification",
}


def find_repo_root() -> Path | None:
    for parent in Path(__file__).resolve().parents:
        if (parent / "examples" / "paper-trading" / "scenario.json").is_file():
            return parent
    return None


def repo_root() -> Path:
    root = find_repo_root()
    if root is None:
        raise RuntimeError("zero-mcp requires a ZERO source checkout for this resource")
    return root


def scenario_path() -> Path:
    return repo_root() / "examples" / "paper-trading" / "scenario.json"


def candles_path() -> Path:
    return repo_root() / "examples" / "paper-trading" / "candles.jsonl"


def proof_pack_path() -> Path:
    return repo_root() / "docs" / "proof" / "demo" / "proof-pack.json"


def load_demo_scenario() -> Any:
    root = find_repo_root()
    if root is None:
        return parse_scenario(EMBEDDED_SCENARIO)
    return load_scenario(root / "examples" / "paper-trading" / "scenario.json")


def market_snapshot(symbols: list[str]) -> JsonMap:
    root = find_repo_root()
    if root is None:
        return {symbol: EMBEDDED_MARKET[symbol] for symbol in symbols}

    from zero_engine import JsonlCandleAdapter

    market = JsonlCandleAdapter(root / "examples" / "paper-trading" / "candles.jsonl")
    return {
        symbol: {
            "as_of": market.latest(symbol).ts,
            "last": market.latest(symbol).close,
        }
        for symbol in symbols
    }


def run_paper_scenario() -> JsonMap:
    scenario = load_demo_scenario()
    engine = PaperEngine(limits=scenario.limits, clock=lambda: PAPER_TS)

    for order in scenario.orders:
        engine.submit(order, source=f"scenario:{scenario.name}")

    symbols = sorted({order.symbol for order in scenario.orders})
    return {
        "schema_version": "zero.mcp.paper_results.v1",
        "mode": scenario.mode,
        "scenario": scenario.name,
        "paper_only": True,
        "market": market_snapshot(symbols),
        "fills": len(engine.fills),
        "rejections": len(engine.rejections),
        "positions": {
            symbol: {
                "quantity": position.quantity,
                "avg_price": position.avg_price,
                "notional_usd": position.notional_usd,
            }
            for symbol, position in sorted(engine.positions.items())
        },
        "decisions": [record.to_dict() for record in engine.decisions],
    }


def list_strategies() -> JsonMap:
    root = find_repo_root()
    runner = None
    if root is not None:
        runner = load_strategy_runner(root / "examples" / "strategy-runner" / "close-strength.yaml")
    return {
        "schema_version": "zero.mcp.strategies.v1",
        "mode": "paper",
        "strategies": [
            {
                "name": "momentum-close-above-open",
                "kind": "built_in",
                "paper_only": True,
                "path": "engine/src/zero_engine/strategy.py",
                "description": "Built-in candle close/open momentum strategy signal.",
            },
            {
                "name": runner.metadata.name if runner is not None else "close-strength-yaml",
                "kind": "declarative_runner",
                "version": runner.metadata.version if runner is not None else "0.1.0",
                "paper_only": runner.metadata.paper_only if runner is not None else True,
                "path": "examples/strategy-runner/close-strength.yaml",
                "description": (
                    runner.metadata.description
                    if runner is not None
                    else "Declarative close-above-open paper runner."
                ),
            },
            {
                "name": "close-strength",
                "kind": "strategy_plugin_example",
                "version": "0.1.0",
                "paper_only": True,
                "path": "examples/strategy-plugin/plugin.py",
                "description": "Smallest contributor path for a deterministic paper strategy plugin.",
            },
        ],
    }


def get_paper_results() -> JsonMap:
    return run_paper_scenario()


def get_position_state() -> JsonMap:
    paper = run_paper_scenario()
    return {
        "schema_version": "zero.mcp.position_state.v1",
        "mode": "paper",
        "paper_only": True,
        "scenario": paper["scenario"],
        "positions": paper["positions"],
    }


def get_proof_pack() -> JsonMap:
    root = find_repo_root()
    if root is None:
        return EMBEDDED_PROOF_PACK
    return json.loads((root / "docs" / "proof" / "demo" / "proof-pack.json").read_text())


def tool_definitions() -> list[JsonMap]:
    empty_schema = {"type": "object", "properties": {}, "additionalProperties": False}
    return [
        {
            "name": "zero_list_strategies",
            "description": "Read-only list of bundled paper strategies and contributor examples.",
            "inputSchema": empty_schema,
        },
        {
            "name": "zero_get_paper_results",
            "description": "Read-only replay of the deterministic bundled paper scenario.",
            "inputSchema": empty_schema,
        },
        {
            "name": "zero_get_position_state",
            "description": "Read-only paper position state derived from the bundled scenario.",
            "inputSchema": empty_schema,
        },
        {
            "name": "zero_get_proof_pack",
            "description": "Read-only public-safe demo proof-pack manifest.",
            "inputSchema": empty_schema,
        },
    ]


TOOLS: dict[str, Callable[[], JsonMap]] = {
    "zero_list_strategies": list_strategies,
    "zero_get_paper_results": get_paper_results,
    "zero_get_position_state": get_position_state,
    "zero_get_proof_pack": get_proof_pack,
}


def resource_definitions() -> list[JsonMap]:
    return [
        {
            "uri": "zero://paper/scenario",
            "name": "Bundled Paper Scenario",
            "description": "The deterministic paper scenario used by examples and MCP tools.",
            "mimeType": "application/json",
        },
        {
            "uri": "zero://paper/results",
            "name": "Bundled Paper Results",
            "description": "Read-only paper replay result generated from the bundled scenario.",
            "mimeType": "application/json",
        },
        {
            "uri": "zero://proof/demo",
            "name": "Demo Proof Pack",
            "description": "Public-safe demo proof-pack manifest.",
            "mimeType": "application/json",
        },
    ]


def read_resource(uri: str) -> str:
    if uri == "zero://paper/scenario":
        root = find_repo_root()
        if root is None:
            return json.dumps(EMBEDDED_SCENARIO, indent=2, sort_keys=True)
        return (root / "examples" / "paper-trading" / "scenario.json").read_text(encoding="utf-8")
    if uri == "zero://paper/results":
        return json.dumps(get_paper_results(), indent=2, sort_keys=True)
    if uri == "zero://proof/demo":
        return json.dumps(get_proof_pack(), indent=2, sort_keys=True)
    raise KeyError(uri)


def text_content(payload: JsonMap) -> list[JsonMap]:
    return [{"type": "text", "text": json.dumps(payload, indent=2, sort_keys=True)}]


def result_response(request_id: Any, result: JsonMap) -> JsonMap:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def error_response(request_id: Any, code: int, message: str) -> JsonMap:
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message},
    }


def requested_protocol(params: JsonMap) -> str:
    version = str(params.get("protocolVersion") or DEFAULT_PROTOCOL_VERSION)
    if version in SUPPORTED_PROTOCOL_VERSIONS:
        return version
    return DEFAULT_PROTOCOL_VERSION


def handle_request(request: JsonMap) -> JsonMap | None:
    method = str(request.get("method", ""))
    request_id = request.get("id")
    params = request.get("params") or {}
    if not isinstance(params, dict):
        return error_response(request_id, -32602, "params must be an object")

    if method == "initialize":
        return result_response(
            request_id,
            {
                "protocolVersion": requested_protocol(params),
                "capabilities": {
                    "tools": {"listChanged": False},
                    "resources": {"subscribe": False, "listChanged": False},
                },
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            },
        )
    if method == "notifications/initialized":
        return None
    if method == "ping":
        return result_response(request_id, {})
    if method == "tools/list":
        return result_response(request_id, {"tools": tool_definitions()})
    if method == "tools/call":
        name = str(params.get("name", ""))
        tool = TOOLS.get(name)
        if tool is None:
            return error_response(request_id, -32602, f"unknown read-only ZERO tool: {name}")
        return result_response(request_id, {"content": text_content(tool()), "isError": False})
    if method == "resources/list":
        return result_response(request_id, {"resources": resource_definitions()})
    if method == "resources/read":
        uri = str(params.get("uri", ""))
        try:
            text = read_resource(uri)
        except KeyError:
            return error_response(request_id, -32602, f"unknown ZERO resource: {uri}")
        return result_response(
            request_id,
            {"contents": [{"uri": uri, "mimeType": "application/json", "text": text}]},
        )
    return error_response(request_id, -32601, f"method not found: {method}")


def serve(stdin: TextIO = sys.stdin, stdout: TextIO = sys.stdout) -> int:
    for raw_line in stdin:
        line = raw_line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError as exc:
            print(json.dumps(error_response(None, -32700, str(exc))), file=stdout, flush=True)
            continue
        if not isinstance(request, dict):
            print(
                json.dumps(error_response(None, -32600, "request must be an object")), file=stdout
            )
            stdout.flush()
            continue
        response = handle_request(request)
        if response is None:
            continue
        print(json.dumps(response, separators=(",", ":")), file=stdout, flush=True)
    return 0


def smoke() -> int:
    tools = tool_definitions()
    resources = resource_definitions()
    forbidden = ("execute", "live_order", "place", "approve")
    names = [tool["name"] for tool in tools]
    if any(marker in name for name in names for marker in forbidden):
        raise RuntimeError(f"zero-mcp exposed a forbidden live execution tool: {names}")
    proof = get_proof_pack()
    boundary = proof.get("claim_boundary", {})
    if boundary.get("live_trading_claimed") or boundary.get("paper_vs_live_correlation_claimed"):
        raise RuntimeError("demo proof pack must not claim live trading or paper/live correlation")
    for name in names:
        payload = TOOLS[name]()
        if not isinstance(payload, dict):
            raise RuntimeError(f"{name} did not return an object")
    print(f"zero mcp smoke passed: {len(tools)} tools, {len(resources)} resources")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="ZERO read-only MCP server")
    parser.add_argument("--smoke", action="store_true", help="validate tool and resource wiring")
    args = parser.parse_args()
    if args.smoke:
        return smoke()
    return serve()


if __name__ == "__main__":
    raise SystemExit(main())
