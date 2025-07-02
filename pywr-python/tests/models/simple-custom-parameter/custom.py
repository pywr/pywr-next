# This is imported only for the type hinting of the `info` argument in the `calc` method.
from pywr import ParameterInfo


class CustomParameter:
    """This is a custom parameter class!

    The arguments to `__init__` are the arguments that are passed to the parameter in the model JSON. This object
    will be initialised once for each scenario in the model. There's no need to handle any state for different
    scenarios.
    """

    def __init__(self, value, multiplier=1.0):
        self.value = value
        self.multiplier = multiplier

    def calc(self, info: ParameterInfo):
        """This method is called to calculate the parameter value for each timestep."""
        return self.value * self.multiplier
