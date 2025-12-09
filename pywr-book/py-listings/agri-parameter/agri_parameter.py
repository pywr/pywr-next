from pathlib import Path


class CropParameter:
    """A simple example of a crop parameter.

    It produces an irrigation requirement value based on the month during `before`. This
    is intended to be used as a demand (or "max_flow") on an irrigation node. The `after`
    method tracks any deficit in irrigation supplied, and at the end of the growing season
    returns the yield for that season.
    """

    def __init__(self):
        self.crop_yield = 0.0
        # Example irrigation requirements by month
        self.irrigation_requirements = {
            1: 0.0,  # January
            2: 0.0,  # February
            3: 10.0,  # March
            4: 20.0,  # April
            5: 30.0,  # May
            6: 40.0,  # June
            7: 30.0,  # July
            8: 20.0,  # August
            9: 10.0,  # September
            10: 0.0,  # October
            11: 0.0,  # November
            12: 0.0  # December
        }
        self.growing_season_months = {3, 4, 5, 6, 7, 8, 9}

    def before(self, info) -> float:
        """Return the irrigation requirement for the current month."""
        return self.irrigation_requirements.get(info.timestep.month, 0.0)

    def after(self, info) -> float:
        """Track the yield based on irrigation supplied."""

        irrigation_required = self.irrigation_requirements.get(info.timestep.month, 0.0)
        irrigation_supplied = info.get_metric("supplied")
        deficit = irrigation_required - irrigation_supplied

        if info.timestep.month not in self.growing_season_months:
            # Reset yield at the end of the growing season
            if info.timestep.month == 10 and info.timestep.day == 1:
                self.crop_yield = 0.0
                return self.crop_yield
        else:
            # Implement a simple crop growth/yield model based on irrigation deficit
            if deficit <= 0:
                self.crop_yield += 1.0  # Full yield increment
            else:
                self.crop_yield += max(0.0, 1.0 - (deficit / irrigation_required))

        return 0.0  # Yield is only returned at the end of the season


def run(model_path: Path):
    from pywr import ModelSchema

    schema = ModelSchema.from_path(model_path)
    model = schema.build(model_path.parent, None)
    model.run("clp")
    print("Model run complete 🎉")


if __name__ == '__main__':
    pth = Path(__file__).parent / "model.json"
    run(pth)
