class FloatParameter:
    """A simple float parameter"""

    def __init__(self, count, *args, **kwargs):
        self.count = count

    def before(self, info) -> float | None:
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)


class IntParameter:
    """A simple int parameter"""

    def __init__(self, count, *args, **kwargs):
        self.count = count

    def before(self, info) -> int | None:
        self.count += info.scenario_index.simulation_id
        return self.count + info.timestep.day


class AfterParameter:
    """A float parameter calculated only after the network is solved"""

    def after(self, info) -> float:
        return float(info.timestep.day)


class BeforeAndAfterParameter:
    """A float parameter calculated both before and after the network is solved"""

    def before(self, info) -> float:
        return float(info.timestep.day)

    def after(self, info) -> float:
        return float(info.timestep.day) + 0.5


def multiple_values(info, factor: float) -> dict:
    """Return multiple values."""
    return {
        "value1": float(info.timestep.index),
        "value2": info.get_metric("deficit") * factor,
    }
