def my_agg_func(values, offset, multiplier=1.0):
    """A simple aggregation function that sums a list of values."""
    return (sum(values) + offset) * multiplier
