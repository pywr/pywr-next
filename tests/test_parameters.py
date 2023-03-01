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
