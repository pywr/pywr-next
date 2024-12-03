from pywr.parameters import ConstantParameter


class MyParameter(ConstantParameter):
    def value(self, *args, **kwargs):
        return 42


MyParameter.register()
