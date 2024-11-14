class FloatParameter:
    """A simple float parameter"""

    def __init__(self, count, *args, **kwargs):
        self.count = 0

    def calc(self, ts, si, p_values) -> float:
        self.count += si
        return float(self.count + ts.day)


class IntParameter:
    """A simple int parameter"""

    def __init__(self, count, *args, **kwargs):
        self.count = 0

    def calc(self, ts, si, p_values) -> int:
        self.count += si
        return self.count + ts.day
