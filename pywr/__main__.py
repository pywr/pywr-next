from .pywr import PyModel  # type: ignore
import random
from itertools import product


def simple1(use_python_parameter=False):
    """Create a very simple three node model."""

    model = PyModel()
    model.add_input_node("my-input")
    model.add_link_node("my-link")
    model.add_output_node("my-output")

    model.connect_nodes("my-input", "my-link")
    model.connect_nodes("my-link", "my-output")

    class ConstantParameter:
        def compute(self):
            return 3.1415

    if use_python_parameter:
        model.add_python_parameter("pi", ConstantParameter())
    else:
        model.add_constant("pi", 3.1415)
    model.set_node_constraint("my-output", "pi")

    model.add_constant("output-cost", -10.0)
    model.set_node_cost("my-output", "output-cost")

    model.run("clp")


class RandomParameter:
    def compute(self):
        return random.random()


def zones(num_zones: int, use_python_parameter=False):
    """Create a model with some interconnected zones."""
    model = PyModel()

    model.add_constant("output-cost", -10.0)

    zones = [f"zone{i:02d}" for i in range(num_zones)]
    for zone in zones:
        model.add_input_node(f"{zone}-input")
        model.add_link_node(f"{zone}-link")
        model.add_output_node(f"{zone}-output")

        model.connect_nodes(f"{zone}-input", f"{zone}-link")
        model.connect_nodes(f"{zone}-link", f"{zone}-output")

        model.set_node_cost(f"{zone}-output", "output-cost")

        if use_python_parameter:
            model.add_python_parameter(f"{zone}-supply", RandomParameter())
        else:
            model.add_constant(f"{zone}-supply", random.random())
        model.add_constant(f"{zone}-demand", random.random())

        model.set_node_constraint(f"{zone}-input", f"{zone}-supply")
        model.set_node_constraint(f"{zone}-output", f"{zone}-demand")

    for zone_from, zone_to in product(zones, zones):
        if zone_from == zone_to:
            continue
        if random.random() < 0.5:
            model.connect_nodes(f"{zone_from}-link", f"{zone_to}-link")

    model.run("clp")


if __name__ == "__main__":
    print("Solving simple1(use_python_parameter=False) ...")
    simple1(use_python_parameter=False)
    print("Solving simple1(use_python_parameter=True) ...")
    simple1(use_python_parameter=True)

    for n in (10, 50, 100):
        print(f"Solving zones({n}, use_python_parameter=False) ...")
        zones(n, use_python_parameter=False)
        print(f"Solving zones({n}, use_python_parameter=True) ...")
        zones(n, use_python_parameter=True)
