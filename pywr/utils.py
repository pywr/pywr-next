from copy import copy
from typing import Optional, Dict
from .nodes import BaseNode
from .model import Model, Timestepper
import logging

from .virtual_storage import BaseVirtualStorageNode

logger = logging.getLogger(__name__)


def pywr_v1_to_v2(v1_data) -> Model:
    """Convert Pywr v1 input data to v2."""
    # First let's categorise the nodes
    v2_node_categories = {}
    for v1_node in v1_data["nodes"]:
        if v1_node["type"].lower() in VIRTUAL_NODE_REMAPPING:
            category = "virtual"
        else:
            category = "node"
        v2_node_categories[v1_node["name"]] = category

    # Convert the nodes first
    v2_nodes = []
    v2_virtual_nodes = []
    for v1_node in v1_data["nodes"]:
        v2_virtual_node = virtual_node_v1_to_v2(v1_node, v2_node_categories)
        if v2_virtual_node is not None:
            v2_virtual_nodes.append(v2_virtual_node)
            continue

        v2_node = node_v1_to_v2(v1_node, v2_node_categories)
        if v2_node is not None:
            v2_nodes.append(v2_node)

    v2_edges = []
    for v1_edge in v1_data["edges"]:
        v2_edge = edge_v1_to_v2(v1_edge)
        v2_edges.append(v2_edge)

    v2_parameters = []
    for v1_parameter_name, v1_parameter in v1_data["parameters"].items():
        v2_parameter = parameter_v1_to_v2(
            v1_parameter_name, v1_parameter, v2_node_categories
        )
        v2_parameters.append(v2_parameter)

    v2_tables = []
    for v1_table_name, v1_table in v1_data["tables"].items():
        print(v1_table)
        v2_table = copy(v1_table)
        v2_table["name"] = v1_table_name
        v2_tables.append(v2_table)

    # print(v2_parameters)
    # for p in v2_parameters:
    #     if p["type"].lower().startswith("aggregated") and "parameters" not in p:
    #         print(p)

    v2_model = Model(
        timestepper=Timestepper(start="1927-01-01", end="2013-12-31", timestep=1),
        nodes=v2_nodes,
        virtual_nodes=v2_virtual_nodes,
        edges=v2_edges,
        parameters=v2_parameters,
        tables=v2_tables,
    )

    return v2_model


NODE_REMAPPING = {
    "reservoir": "storage",
    "river": "link",
    "transfer": "link",
}

VIRTUAL_NODE_REMAPPING = {
    "annualvirtualstorage": "virtualstorage",
    "rollingvirtualstorage": "rollingvirtualstorage",
}


def inline_parameters_v1_to_v2(
    data, parent_name: str, v2_node_categories: Dict[str, str]
):
    for k, v in data.items():
        if isinstance(v, dict) and "type" in v:
            # Inline parameter
            name = f"{parent_name}-{k}"
            v_new = parameter_v1_to_v2(name, v, v2_node_categories)
        elif isinstance(v, list):
            # Could be a list of inline parameters
            v_new = []
            for i, v2 in enumerate(v):
                if isinstance(v2, dict):
                    name = f"{parent_name}-{k}-{i:02}"
                    v2_new = parameter_v1_to_v2(name, v2, v2_node_categories)
                else:
                    v2_new = v2
                v_new.append(v2_new)
        else:
            v_new = v
        data[k] = v_new


def node_v1_to_v2(v1_node_data, v2_node_categories: Dict[str, str]) -> Optional[Dict]:
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
    v2_node_name = v2_node_data["name"]
    inline_parameters_v1_to_v2(v2_node_data, v2_node_name, v2_node_categories)

    return v2_node_data


def virtual_node_v1_to_v2(
    v1_node_data, v2_node_categories: Dict[str, str]
) -> Optional[Dict]:
    node_type = v1_node_data["type"].lower()

    if node_type in VIRTUAL_NODE_REMAPPING:
        node_type = VIRTUAL_NODE_REMAPPING[node_type]

    try:
        klass = BaseVirtualStorageNode.get_class(node_type)
    except KeyError:
        return None

    v2_node_data = copy(v1_node_data)
    v2_node_data["type"] = node_type
    v2_node_name = v2_node_data["name"]
    inline_parameters_v1_to_v2(v2_node_data, v2_node_name, v2_node_categories)

    return v2_node_data


def edge_v1_to_v2(v1_edge_data) -> Optional[Dict]:
    from_node = v1_edge_data[0]
    to_node = v1_edge_data[1]

    return dict(from_node=from_node, to_node=to_node)


def parameter_v1_to_v2(
    v1_parameter_name, v1_parameter, v2_node_categories: Dict[str, str]
) -> Optional[Dict]:
    p_type = v1_parameter["type"].lower()
    print(p_type)
    if p_type.startswith("constant"):
        v2_parameter = constant_v1_to_v2(v1_parameter)
    elif p_type.startswith("monthlyprofile"):
        v2_parameter = monthly_profile_v1_to_v2(v1_parameter)
    elif p_type.startswith("dailyprofile"):
        v2_parameter = daily_profile_v1_to_v2(v1_parameter)
    elif p_type.startswith("indexedarray"):
        v2_parameter = indexed_array_v1_to_v2(v1_parameter)
    elif p_type.startswith("polynomial1d"):
        v2_parameter = polynomial_1d_v1_to_v2(v1_parameter)
    elif p_type.startswith("parameterthreshold"):
        v2_parameter = threshold_1d_v1_to_v2(v1_parameter)
    elif p_type in (
        "controlcurve",
        "controlcurveparameter",
        "controlcurvepiecewiseinterpolated",
        "controlcurvepiecewiseinterpolatedparameter",
        "controlcurveindex",
        "controlcurveindexparameter",
        "controlcurveinterpolated",
        "controlcurveinterpolatedparameter",
    ):
        v2_parameter = control_curve_v1_to_v2(v1_parameter, v2_node_categories)
    else:
        v2_parameter = copy_v1_to_v2(v1_parameter)

    if v2_parameter["type"].lower().endswith("parameter"):
        v2_parameter["type"] = v2_parameter["type"][: -len("parameter")]
    v2_parameter["name"] = v1_parameter_name
    inline_parameters_v1_to_v2(v2_parameter, v1_parameter_name, v2_node_categories)

    return v2_parameter


def constant_v1_to_v2(v1_parameter) -> Dict:
    v2_parameter = copy(v1_parameter)
    if "values" in v2_parameter:
        v2_parameter["value"] = v2_parameter.pop("values")
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


def daily_profile_v1_to_v2(v1_parameter) -> Dict:
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


def polynomial_1d_v1_to_v2(v1_parameter) -> Dict:
    v2_parameter = copy(v1_parameter)
    if "node" in v2_parameter:
        metric = "node_inflow"
        name = v2_parameter.pop("node")
    elif "storage_node" in v2_parameter:
        metric = (
            "node_proportional_volume"
            if v2_parameter.pop("use_proportional_volume", False)
            else "node_volume"
        )
        name = v2_parameter.pop("storage_node")
    elif "parameter" in v2_parameter:
        metric = "parameter_value"
        name = v2_parameter.pop("parameter")
    else:
        raise ValueError("No valid component found.")
    v2_parameter["metric"] = {"metric_type": metric, "name": name, "component": None}
    return v2_parameter


def threshold_1d_v1_to_v2(v1_parameter) -> Dict:
    v2_parameter = copy(v1_parameter)
    if "node" in v2_parameter:
        metric = "node_inflow"
        name = v2_parameter.pop("node")
    elif "storage_node" in v2_parameter:
        metric = (
            "node_volume"
            if v2_parameter.pop("use_proportional_volume", False)
            else "node_proportional_volume"
        )
        name = v2_parameter.pop("storage_node")
    elif "parameter" in v2_parameter:
        metric = "parameter_value"
        name = v2_parameter.pop("parameter")
    else:
        raise ValueError("No valid component found.")
    v2_parameter["metric"] = {"metric_type": metric, "name": name, "component": None}
    v2_parameter["threshold"] = {
        "metric_type": "constant_float",
        "name": None,
        "component": None,
        "value": v2_parameter["threshold"],
    }
    v2_parameter["type"] = "threshold"
    return v2_parameter


def control_curve_v1_to_v2(v1_parameter, v2_node_categories: Dict[str, str]) -> Dict:
    v2_parameter = copy(v1_parameter)

    name = v2_parameter.pop("storage_node")

    category = v2_node_categories.get(name)
    if category == "node":
        metric_type = "node_proportional_volume"
    elif category == "virtual":
        metric_type = "virtual_storage_proportional_volume"
    else:
        raise ValueError(f"Invalid node category: {category}")

    v2_parameter["metric"] = {
        "metric_type": metric_type,
        "name": name,
        "component": None,
    }
    if "control_curve" in v2_parameter:
        v2_parameter["control_curves"] = [
            v2_parameter.pop("control_curve"),
        ]
    if "parameters" in v2_parameter:
        v2_parameter["values"] = v2_parameter.pop("parameters")
    return v2_parameter


def copy_v1_to_v2(v1_parameter):
    v2_parameter = copy(v1_parameter)
    if "parameter" in v2_parameter:
        if isinstance(v2_parameter["parameter"], str):
            v2_parameter["metric"] = {
                "metric_type": "parameter_value",
                "name": v2_parameter.pop("parameter"),
                "component": None,
            }
        else:
            v2_parameter["metric"] = v2_parameter.pop("parameter")
    return v2_parameter
