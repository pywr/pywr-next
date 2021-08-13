import numpy as np
from scipy import stats
import pytest

from pywr.nodes import (
    BaseNode,
    InputNode,
    LinkNode,
    OutputNode,
    Edge,
    Model,
    Timestepper,
)
from pywr.parameters.base import ArrayIndexParameter


@pytest.fixture()
def default_timestepper() -> Timestepper:
    return Timestepper(start="2021-01-01", end="2021-12-31", timestep=1)


def make_simple_system(
    model: Model, suffix: str, input_loc=9, input_scale=1, output_loc=8, output_scale=3
):
    max_flow = stats.norm.rvs(loc=input_loc, scale=input_scale, size=365 * 12)
    max_flow[max_flow < 0] = 0.0

    inflow = ArrayIndexParameter(name=f"inflow-{suffix}", values=max_flow.tolist())

    input_node = InputNode(name=f"input-{suffix}", max_flow=inflow.name, cost=-10.0)

    link_node = LinkNode(name=f"link-{suffix}")

    max_flow = stats.norm.rvs(loc=output_loc, scale=output_scale, size=1)[0]
    max_flow = max(max_flow, 0)

    output_node = OutputNode(name=f"output-{suffix}", max_flow=max_flow, cost=-500.0)

    model.nodes.append(input_node)
    model.nodes.append(link_node)
    model.nodes.append(output_node)
    model.edges.append(Edge(from_node=input_node.name, to_node=link_node.name))
    model.edges.append(Edge(from_node=link_node.name, to_node=output_node.name))
    model.parameters.append(inflow)


def make_simple_connections(
    model: Model,
    number_of_systems: int,
    density: int = 10,
    loc: int = 15,
    scale: int = 5,
):
    num_connections = (number_of_systems ** 2) * density // 100 // 2

    connections_added = 0

    while connections_added < num_connections:
        i, j = np.random.randint(number_of_systems, size=2)
        mf = stats.norm.rvs(loc=loc, scale=scale, size=1)
        name = "transfer-{}-{}".format(i, j)

        if name in model.nodes or i == j:
            continue

        link = LinkNode(name=name, max_flow=max(mf, 0), cost=1)

        from_name = f"link-{i:02d}"
        to_node = f"link-{j:02d}"

        model.nodes.append(link)
        model.edges.append(Edge(from_node=from_name, to_node=link.name))
        model.edges.append(Edge(from_node=link.name, to_node=to_node))
        connections_added += 1


@pytest.mark.parametrize(
    "number_of_systems,connection_density",
    [
        (5, 5),
        (10, 5),
        (20, 5),
        (30, 5),
        (40, 5),
        (50, 5),
    ],
)
def test_random_model(
    benchmark, default_timestepper, number_of_systems, connection_density
):

    model = Model(
        timestepper=default_timestepper,
    )
    for i in range(number_of_systems):
        make_simple_system(model, f"{i:02d}")

    make_simple_connections(model, number_of_systems, density=connection_density)

    r_model = model.build()

    benchmark(
        r_model.run,
        "clp",
        model.timestepper.start,
        model.timestepper.end,
        model.timestepper.timestep,
    )


if __name__ == "__main__":

    def benchmark(func, *args, **kwargs):
        func(*args, **kwargs)

    test_random_model(
        benchmark, Timestepper(start="2021-01-01", end="2031-12-31", timestep=1), 50, 5
    )
