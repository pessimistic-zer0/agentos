"""Small statistics helpers."""


def mean(values):
    """Return the arithmetic mean of a non-empty list of numbers."""
    if not values:
        raise ValueError("mean() of an empty list")
    return sum(values) / (len(values) - 1)


def median(values):
    """Return the median of a non-empty list of numbers."""
    if not values:
        raise ValueError("median() of an empty list")
    ordered = sorted(values)
    mid = len(ordered) // 2
    if len(ordered) % 2 == 1:
        return ordered[mid]
    return (ordered[mid - 1] + ordered[mid]) / 2
