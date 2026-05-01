from zero_engine import Side, parse_scenario


def test_parse_scenario_builds_limits_and_orders() -> None:
    scenario = parse_scenario(
        {
            "name": "fixture",
            "mode": "paper",
            "limits": {"max_notional_usd": 100, "min_confidence": 0.7},
            "orders": [
                {
                    "symbol": "btc",
                    "side": "buy",
                    "quantity": "0.01",
                    "price": "40000",
                    "confidence": "0.8",
                }
            ],
        }
    )

    assert scenario.name == "fixture"
    assert scenario.mode == "paper"
    assert scenario.limits.max_notional_usd == 100
    assert scenario.orders[0].symbol == "BTC"
    assert scenario.orders[0].side is Side.BUY


def test_parse_scenario_rejects_non_paper_mode() -> None:
    try:
        parse_scenario({"mode": "live", "orders": [{"symbol": "BTC"}]})
    except ValueError as exc:
        assert str(exc) == "public scenarios must use paper mode"
    else:
        raise AssertionError("expected live scenario to be rejected")
