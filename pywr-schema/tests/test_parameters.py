class FloatParameter:
    """A simple float parameter"""

    def __init__(self, count, *args, **kwargs):
        self.count = count

    def calc(self, info) -> float:
        self.count += info.scenario_index.simulation_id
        return float(self.count + info.timestep.day)


class IntParameter:
    """A simple int parameter"""

    def __init__(self, count, *args, **kwargs):
        self.count = count

    def calc(self, info) -> int:
        self.count += info.scenario_index.simulation_id
        return self.count + info.timestep.day


def multiple_values(info, factor: float) -> dict:
    """Return multiple values."""
    return {
        "value1": float(info.timestep.index),
        "value2": info.get_metric("deficit") * factor,
    }
