"""Search for good values of a model's variable parameters using random sampling.

This example uses a simple random search to show how a `ModelProblem` can be used by an
optimisation algorithm. In practice, a library of optimisation algorithms (e.g. multi-objective
evolutionary algorithms) would be used to choose the values to evaluate.
"""

import random
from pathlib import Path

from pywr import Evaluation, ModelProblem, ModelSchema, ProblemConfig


# ANCHOR: build
def build_problem() -> ModelProblem:
    here = Path(__file__).parent

    schema = ModelSchema.from_path(here / "model.json")
    config = ProblemConfig.from_path(here / "problem.json")

    return schema.build_problem(config, data_path=here)


# ANCHOR_END: build


def dominates(a: list[float], b: list[float]) -> bool:
    """Whether objective values `a` dominate `b`, when all objectives are minimised."""
    return all(x <= y for x, y in zip(a, b, strict=True)) and any(
        x < y for x, y in zip(a, b, strict=True)
    )


# ANCHOR: search
def random_search(
    problem: ModelProblem, num_samples: int, seed: int = 42
) -> list[tuple[list[float], Evaluation]]:
    """Evaluate random values, and return the feasible values that are not dominated."""
    rng = random.Random(seed)
    lower, _ = problem.lower_bounds()
    upper, _ = problem.upper_bounds()

    samples = [
        [rng.uniform(lb, ub) for lb, ub in zip(lower, upper, strict=True)]
        for _ in range(num_samples)
    ]

    # Each sample is evaluated by running the model; the samples are run in parallel.
    evaluations = problem.evaluate_batch(samples)

    # Negate the objectives that are maximised, so that all objectives are minimised.
    signs = [1.0 if sense == "Minimise" else -1.0 for sense in problem.objective_senses]

    def minimised(evaluation: Evaluation) -> list[float]:
        return [s * o for s, o in zip(signs, evaluation.objectives, strict=True)]

    feasible = [
        (sample, evaluation)
        for sample, evaluation in zip(samples, evaluations, strict=True)
        if evaluation.is_feasible
    ]

    return [
        (sample, evaluation)
        for sample, evaluation in feasible
        if not any(
            dominates(minimised(other), minimised(evaluation)) for _, other in feasible
        )
    ]


# ANCHOR_END: search


if __name__ == "__main__":
    problem = build_problem()
    print(problem)

    results = random_search(problem, num_samples=50)
    for sample, evaluation in sorted(results, key=lambda result: result[0]):
        objectives = ", ".join(
            f"{name}={value:.2f}"
            for name, value in zip(
                problem.objective_names, evaluation.objectives, strict=True
            )
        )
        print(f"demand={sample[0]:.2f}: {objectives}")
