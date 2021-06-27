from typing import Optional, Dict, List
from pydantic import BaseModel
from .pywr import PyModel  # type: ignore

_recorder_registry = {}


class BaseRecorder(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _recorder_registry[cls.__name__.lower()] = cls

    def create_recorder(self, r_model: PyModel):
        raise NotImplementedError()


class HDF5Recorder(BaseRecorder):
    def create_recorder(self, r_model: PyModel):
        r_model.add_hdf5_recorder(self.name, self.comment)


class AssertionRecorder(BaseRecorder):
    component: str
    metric: str
    values: List[float]

    def create_recorder(self, r_model: PyModel):
        r_model.add_python_recorder(
            self.name, self.component, self.metric, _AssertionRecorder(self.values)
        )


class _AssertionRecorder:
    def __init__(self, values: List[float]):
        self._iter = iter(values)

    def save(self, timestep, value: float):
        print(timestep, value)

        assert next(self._iter) == value


class RecorderCollection:
    def __init__(self):
        self._recorders: Dict[str, BaseRecorder] = {}

    def __getitem__(self, item: str):
        return self._recorders[item]

    def __setitem__(self, key: str, value: BaseRecorder):
        self._recorders[key] = value

    def __iter__(self):
        return iter(self._recorders.values())

    def __len__(self):
        return len(self._recorders)

    def __contains__(self, item):
        return item in self._recorders

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, list):
            raise TypeError("list required")

        collection = cls()
        for recorder_data in data:
            collection.add(**recorder_data)

        return collection

    def add(self, **data):
        if "type" not in data:
            raise ValueError('"type" key required')

        klass_name = data.pop("type") + "recorder"
        klass = _recorder_registry[klass_name]
        recorder = klass(**data)
        if recorder.name in self:
            raise ValueError(f"Recorder name {recorder.name} already defined.")
        self[recorder.name] = recorder
