from copy import copy
from typing import Optional, Dict
from .nodes import BaseNode
from .model import Model, Timestepper
import logging

logger = logging.getLogger(__name__)


def pywr_v1_to_v2(v1_data) -> Model:
    """Convert Pywr v1 input data to v2."""

    # Convert the nodes first
    v2_nodes = []
    for v1_node in v1_data["nodes"]:
        v2_node = node_v1_to_v2(v1_node)
        if v2_node is not None:
            v2_nodes.append(v2_node)

    v2_edges = []
    for v1_edge in v1_data["edges"]:
        v2_edge = edge_v1_to_v2(v1_edge)
        v2_edges.append(v2_edge)

    v2_parameters = []
    for v1_parameter_name, v1_parameter in v1_data["parameters"].items():
        v2_parameter = parameter_v1_to_v2(v1_parameter_name, v1_parameter)
        v2_parameters.append(v2_parameter)

    # print(v2_parameters)
    for p in v2_parameters:
        if p["type"].lower().startswith("aggregated") and "parameters" not in p:
            print(p)

    v2_model = Model(
        timestepper=Timestepper(start="2021-01-01", end="2021-12-31", timestep=1),
        nodes=v2_nodes,
        edges=v2_edges,
        parameters=v2_parameters,
    )

    return v2_model


NODE_REMAPPING = {
    "losslink": "link",
    "reservoir": "storage",
    "catchment": "input",
    "river": "link",
    "rivergauge": "link",
    "riversplitwithgauge": "link",
    "transfer": "link",
}


def node_v1_to_v2(v1_node_data) -> Optional[Dict]:
    node_type = v1_node_data["type"].lower()

    if node_type in NODE_REMAPPING:
        node_type = NODE_REMAPPING[node_type]

    try:
        klass = BaseNode.get_class(node_type)
    except KeyError:
        logger.warning(f'Unknown v1 node type: "{node_type}".')
        return None

    v2_node_data = copy(v1_node_data)
    v2_node_data["type"] = node_type

    return v2_node_data


def edge_v1_to_v2(v1_edge_data) -> Optional[Dict]:
    from_node = v1_edge_data[0]
    to_node = v1_edge_data[1]

    return dict(from_node=from_node, to_node=to_node)


def parameter_v1_to_v2(v1_parameter_name, v1_parameter) -> Optional[Dict]:
    p_type = v1_parameter["type"].lower()

    if p_type.startswith("constant"):
        v2_parameter = constant_v1_to_v2(v1_parameter)
    elif p_type.startswith("monthlyprofile"):
        v2_parameter = monthly_profile_v1_to_v2(v1_parameter)
    elif p_type.startswith("indexedarray"):
        v2_parameter = indexed_array_v1_to_v2(v1_parameter)
    else:
        v2_parameter = copy(v1_parameter)

    if v2_parameter["type"].lower().endswith("parameter"):
        v2_parameter["type"] = v2_parameter["type"][: -len("parameter")]
    v2_parameter["name"] = v1_parameter_name
    return v2_parameter


def constant_v1_to_v2(v1_parameter) -> Dict:
    v2_parameter = copy(v1_parameter)
    if "value" not in v2_parameter:
        value = {}
        for key in ("table", "index", "column"):
            if key in v2_parameter:
                value[key] = v2_parameter.pop(key)
        v2_parameter["value"] = value
    return v2_parameter


def monthly_profile_v1_to_v2(v1_parameter) -> Dict:
    v2_parameter = copy(v1_parameter)
    if "values" not in v2_parameter:
        values = {}
        for key in ("table", "index", "column", "index_col", "url"):
            if key in v2_parameter:
                values[key] = v2_parameter.pop(key)
        v2_parameter["values"] = values
    return v2_parameter


def indexed_array_v1_to_v2(v1_parameter) -> Dict:
    v2_parameter = copy(v1_parameter)
    if "params" in v2_parameter:
        v2_parameter["parameters"] = v2_parameter.pop("params")
    return v2_parameter
