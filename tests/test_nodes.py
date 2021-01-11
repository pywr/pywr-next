from pywr.nodes import Model, InputNode, LinkNode, OutputNode, NodeCollection
import pytest


def test_simple_schema():
    """Test basic """

    data = {
        "nodes": [
            {"name": "input1", "type": "input"},
            {"name": "link1", "type": "link"},
            {"name": "output1", "type": "output", "cost": -10.0, "max_flow": "demand"},
        ],
        "edges": [
            {"from_node": "input1", "to_node": "link1"},
            {"from_node": "link1", "to_node": "output1"},
        ],
        "parameters": [{"name": "demand", "type": "constant", "value": 10.0}],
    }

    model = Model(**data)
    assert len(model.nodes) == 3
    assert len(model.edges) == 2
    assert len(model.parameters) == 1
    assert isinstance(model.nodes["input1"], InputNode)
    assert isinstance(model.nodes["link1"], LinkNode)
    assert isinstance(model.nodes["output1"], OutputNode)

    model.run()

    # TODO test the outputs


def test_duplicate_node_name_error():

    data = {
        "nodes": [
            {"name": "node1", "type": "input"},
            {"name": "node1", "type": "link"},
        ],
        "edges": [],
        "parameters": [],
    }

    with pytest.raises(ValueError):
        Model(**data)
