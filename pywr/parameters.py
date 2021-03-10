from typing import Optional, Dict
from pydantic import BaseModel
from .pywr import PyModel  # type: ignore

_parameter_registry = {}


class BaseParameter(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _parameter_registry[cls.__name__.lower()] = cls

    def create_parameter(self, r_model: PyModel):
        raise NotImplementedError()


class ConstantParameter(BaseParameter):
    value: float

    def create_parameter(self, r_model: PyModel):
        r_model.add_constant(self.name, self.value)


class ParameterCollection:
    def __init__(self):
        self._parameters: Dict[str, BaseParameter] = {}

    def __getitem__(self, item: str):
        return self._parameters[item]

    def __setitem__(self, key: str, value: BaseParameter):
        self._parameters[key] = value

    def __iter__(self):
        return iter(self._parameters.values())

    def __len__(self):
        return len(self._parameters)

    def __contains__(self, item):
        return item in self._parameters

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, list):
            raise TypeError("list required")

        collection = cls()
        for parameter_data in data:

            if "type" not in parameter_data:
                raise ValueError('"type" key required')

            klass_name = parameter_data.pop("type") + "parameter"
            klass = _parameter_registry[klass_name]
            parameter = klass(**parameter_data)
            if parameter.name in collection:
                raise ValueError(f"Parameter name {parameter.name} already defined.")
            collection[parameter.name] = parameter
        return collection
