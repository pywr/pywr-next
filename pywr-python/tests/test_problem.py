import json
from pathlib import Path

import pytest
from pywr import (
    Evaluation,
    ModelProblem,
    ModelSchema,
    MultiNetworkModelProblem,
    MultiNetworkModelSchema,
    ProblemBuildError,
    ProblemConfig,
    SetupError,
)


@pytest.fixture()
def problem_dir() -> Path:
    return Path(__file__).parent / "models" / "simple-problem"


@pytest.fixture()
def problem(problem_dir: Path) -> ModelProblem:
    schema = ModelSchema.from_path(problem_dir / "model.json")
    config = ProblemConfig.from_path(problem_dir / "problem.json")
    return schema.build_problem(config, data_path=problem_dir)


def test_problem_definition(problem: ModelProblem):
    """Test the problem's variables, objectives and constraints."""
    assert problem.size == (1, 0)
    assert problem.lower_bounds() == ([0.0], [])
    assert problem.upper_bounds() == ([20.0], [])

    assert problem.objective_names == ["mean-demand", "mean-delivered"]
    assert problem.objective_senses == ["Minimise", "Maximise"]
    assert problem.constraint_names == ["mean-delivered"]
    assert problem.constraint_bounds == [(5.0, None)]


def test_evaluate(problem: ModelProblem):
    """Test evaluating the problem with feasible and infeasible values."""
    # The supply is limited to 8.0, so the delivered flow is capped
    evaluation = problem.evaluate([10.0])
    assert isinstance(evaluation, Evaluation)
    assert evaluation.objectives == pytest.approx([10.0, 8.0])
    assert evaluation.constraints == pytest.approx([8.0])
    assert evaluation.constraint_violations == pytest.approx([0.0])
    assert evaluation.is_feasible

    # A demand of 4.0 is below the minimum delivered flow of 5.0
    evaluation = problem.evaluate([4.0])
    assert evaluation.objectives == pytest.approx([4.0, 4.0])
    assert evaluation.constraint_violations == pytest.approx([1.0])
    assert not evaluation.is_feasible


def test_evaluate_batch(problem: ModelProblem):
    """Test evaluating several sets of values in parallel returns results in order."""
    demands = [2.0, 6.0, 8.0, 12.0]
    evaluations = problem.evaluate_batch([[d] for d in demands])

    assert len(evaluations) == len(demands)
    for demand, evaluation in zip(demands, evaluations, strict=True):
        assert evaluation.objectives == pytest.approx([demand, min(demand, 8.0)])


def test_evaluate_incorrect_number_of_values(problem: ModelProblem):
    with pytest.raises(SetupError):
        problem.evaluate([1.0, 2.0])

    with pytest.raises(ValueError):
        problem.evaluate_batch([[1.0], [2.0]], values_u64=[[]])


def test_problem_config_round_trip(problem_dir: Path):
    config = ProblemConfig.from_path(problem_dir / "problem.json")
    data = json.loads(config.to_json_string())

    assert [v["name"] for v in data["variables"]] == ["demand"]
    assert ProblemConfig.from_json_string(config.to_json_string()) is not None


def test_build_problem_missing_output(problem_dir: Path):
    schema = ModelSchema.from_path(problem_dir / "model.json")
    config = ProblemConfig.from_json_string(
        json.dumps(
            {
                "variables": [],
                "objectives": [{"output": "missing", "sense": "Minimise"}],
            }
        )
    )

    with pytest.raises(ProblemBuildError):
        schema.build_problem(config, data_path=problem_dir)


@pytest.fixture()
def multi_problem_dir() -> Path:
    return Path(__file__).parent / "models" / "multi-problem"


@pytest.fixture()
def multi_problem(multi_problem_dir: Path) -> MultiNetworkModelProblem:
    schema = MultiNetworkModelSchema.from_path(multi_problem_dir / "model.json")
    config = ProblemConfig.from_path(multi_problem_dir / "problem.json")
    return schema.build_problem(config, data_path=multi_problem_dir)


def test_multi_network_problem_definition(multi_problem: MultiNetworkModelProblem):
    """Test the variables, objectives and constraints of a multi-network problem."""
    # The variables are in the order they are listed: the demand of network2, then network1
    assert multi_problem.size == (2, 0)
    assert multi_problem.lower_bounds() == ([0.0, 0.0], [])
    assert multi_problem.upper_bounds() == ([30.0, 20.0], [])

    assert multi_problem.objective_names == ["mean-delivered", "mean-delivered"]
    assert multi_problem.objective_networks == ["network1", "network2"]
    assert multi_problem.objective_senses == ["Maximise", "Maximise"]
    assert multi_problem.constraint_names == ["mean-delivered"]
    assert multi_problem.constraint_networks == ["network2"]
    assert multi_problem.constraint_bounds == [(5.0, None)]


def test_multi_network_evaluate(multi_problem: MultiNetworkModelProblem):
    """Test evaluating a multi-network problem, where network2 is supplied by network1."""
    # network1 delivers its demand of 10.0, of which network2 takes its demand of 8.0
    evaluation = multi_problem.evaluate([8.0, 10.0])
    assert evaluation.objectives == pytest.approx([10.0, 8.0])
    assert evaluation.is_feasible

    # network1 delivers only 4.0, which limits network2 to below its minimum of 5.0
    evaluation = multi_problem.evaluate([12.0, 4.0])
    assert evaluation.objectives == pytest.approx([4.0, 4.0])
    assert evaluation.constraint_violations == pytest.approx([1.0])
    assert not evaluation.is_feasible

    evaluations = multi_problem.evaluate_batch([[8.0, 10.0], [12.0, 4.0]])
    assert [e.objectives for e in evaluations] == [
        pytest.approx([10.0, 8.0]),
        pytest.approx([4.0, 4.0]),
    ]


def test_multi_network_build_problem_network_required(multi_problem_dir: Path):
    schema = MultiNetworkModelSchema.from_path(multi_problem_dir / "model.json")
    config = ProblemConfig.from_json_string(
        json.dumps(
            {
                "variables": [],
                "objectives": [{"output": "mean-delivered", "sense": "Maximise"}],
            }
        )
    )

    with pytest.raises(ProblemBuildError):
        schema.build_problem(config, data_path=multi_problem_dir)
