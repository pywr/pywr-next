import pytest
import numpy as np
from pywr.model import Model


@pytest.fixture()
def simple_data():
    return {
        "timestepper": {"start": "2020-01-01", "end": "2020-12-31", "timestep": 1},
        "nodes": [
            {"name": "input1", "type": "input"},
            {"name": "link1", "type": "link"},
            {
                "name": "output1",
                "type": "output",
                "cost": -10.0,
                "max_flow": 10.0,
            },
        ],
        "edges": [
            {"from_node": "input1", "to_node": "link1"},
            {"from_node": "link1", "to_node": "output1"},
        ],
        "parameters": [],
    }


@pytest.fixture()
def simple_storage_data():
    return {
        "timestepper": {"start": "2020-01-01", "end": "2020-01-31", "timestep": 1},
        "nodes": [
            {"name": "input1", "type": "input"},
            {
                "name": "storage1",
                "type": "storage",
                "max_volume": 20,
                "initial_volume": 20,
            },
            {
                "name": "output1",
                "type": "output",
                "cost": -10.0,
                "max_flow": 1.0,
            },
        ],
        "edges": [
            {"from_node": "input1", "to_node": "storage1"},
            {"from_node": "storage1", "to_node": "output1"},
        ],
        "parameters": [],
    }


class TestAggregatedParameter:
    __test_funcs__ = {
        "sum": np.sum,
        "product": np.product,
        "mean": np.mean,
        "max": np.max,
        "min": np.min,
    }

    @pytest.mark.parametrize("agg_func", ["sum", "product", "mean", "max", "min"])
    def test_two_parameters(self, simple_data, agg_func):
        """Test an aggregated node with two parameters."""
        test_func = self.__test_funcs__[agg_func]
        simple_data["parameters"] = [
            {"name": "p1", "type": "constant", "value": 10.0},
            {"name": "p2", "type": "constant", "value": 10.0},
            {
                "name": "agg",
                "type": "aggregated",
                "agg_func": agg_func,
                "parameters": ["p1", "p2"],
            },
        ]

        model = Model(**simple_data)
        model.recorders.add(
            **{
                "name": "assert",
                "type": "assertion",
                "component": "agg",
                "metric": "parameter",
                "values": [test_func([10.0, 10.0])] * 366,
            }
        )
        assert len(model.parameters) == 3

        model.run()

    def test_ordering(self, simple_data):
        """Test that a model loads if the aggregated parameter is defined before its dependencies."""

        simple_data["parameters"] = [
            {
                "name": "agg",
                "type": "aggregated",
                "agg_func": "sum",
                "parameters": ["p1", "p2"],
            },
            {"name": "p1", "type": "constant", "value": 10.0},
            {"name": "p2", "type": "constant", "value": 10.0},
        ]

        model = Model(**simple_data)
        assert len(model.parameters) == 3

        model.run()

    def test_cycle_error(self, simple_data):
        """Test that a cycle in parameter dependencies does not load."""

        simple_data["parameters"] = [
            {
                "name": "agg1",
                "type": "aggregated",
                "agg_func": "sum",
                "parameters": ["p1", "agg2"],
            },
            {"name": "p1", "type": "constant", "value": 10.0},
            {
                "name": "agg2",
                "type": "aggregated",
                "agg_func": "sum",
                "parameters": ["p1", "agg1"],
            },
        ]

        model = Model(**simple_data)
        assert len(model.parameters) == 3

        with pytest.raises(RuntimeError):
            model.run()


class TestControlCurvePiecewiseInterpolatedParameter:
    def test_basic(self, simple_storage_data):
        """Basic functional test of `ControlCurvePiecewiseInterpolatedParameter`"""

        simple_storage_data["parameters"] = [
            {"name": "cc1", "type": "constant", "value": 0.8},
            {"name": "cc2", "type": "constant", "value": 0.5},
            {
                "name": "cc_interp",
                "type": "ControlCurvePiecewiseInterpolated",
                "storage_node": "storage1",
                "control_curves": ["cc1", "cc2"],
                "values": [
                    [10.0, 1.0],
                    [0.0, 0.0],
                    [-1.0, -10.0],
                ],
            },
        ]

        model = Model(**simple_storage_data)
        model.recorders.add(
            **{
                "name": "assert",
                "type": "assertion",
                "component": "cc_interp",
                "metric": "parameter",
                "values": [
                    10.0,  # 20 Ml (full)
                    1.0 + 9.0 * 0.15 / 0.2,  # 19 Ml (95%)
                    1.0 + 9.0 * 0.10 / 0.2,  # 18 Ml (90%)
                    1.0 + 9.0 * 0.05 / 0.2,  # 17 Ml (85%)
                    0.0,  # 16 Ml (80%)
                    0.0,  # 15 Ml (75%)
                    0.0,  # 14 Ml (70%)
                    0.0,  # 13 Ml (65%)
                    0.0,  # 12 Ml (60%)
                    0.0,  # 11 Ml (55%)
                    -1.0,  # 10 Ml (50%)
                    -1.0 - 9.0 * 0.05 / 0.5,  # 09 Ml (45%)
                    -1.0 - 9.0 * 0.10 / 0.5,  # 09 Ml (40%)
                    -1.0 - 9.0 * 0.15 / 0.5,  # 09 Ml (35%)
                    -1.0 - 9.0 * 0.20 / 0.5,  # 09 Ml (30%)
                    -1.0 - 9.0 * 0.25 / 0.5,  # 09 Ml (25%)
                    -1.0 - 9.0 * 0.30 / 0.5,  # 09 Ml (20%)
                    -1.0 - 9.0 * 0.35 / 0.5,  # 09 Ml (15%)
                    -1.0 - 9.0 * 0.40 / 0.5,  # 09 Ml (10%)
                    -1.0 - 9.0 * 0.45 / 0.5,  # 09 Ml (05%)
                    -10.0,  # 09 Ml (00%)
                ]
                + [-10.0] * 10,
            }
        )
        assert len(model.parameters) == 3

        model.run()
