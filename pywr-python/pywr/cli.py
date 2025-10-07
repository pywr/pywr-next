import logging
from pathlib import Path
from typing import Optional

import click
from . import run_from_path


def configure_logging():
    logger = logging.getLogger("pywr")
    logger.setLevel(logging.INFO)

    ch = logging.StreamHandler()
    formatter = logging.Formatter(
        "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    )
    ch.setFormatter(formatter)
    logger.addHandler(ch)


@click.group()
def cli():
    configure_logging()


@cli.command()
@click.argument("path", type=click.Path(exists=True, file_okay=True))
@click.option(
    "-s", "--solver", type=click.Choice(["clp", "highs", "cbc"]), default="clp"
)
@click.option(
    "-d", "--data-path", type=click.Path(exists=True, dir_okay=True), default=None
)
@click.option(
    "-o", "--output-path", type=click.Path(exists=True, dir_okay=True), default=None
)
def run(path: str, solver: str, data_path: Optional[str], output_path: Optional[str]):
    data_path = Path(data_path) if data_path is not None else None
    output_path = Path(output_path) if output_path is not None else None

    run_from_path(
        Path(path),
        solver=solver,
        data_path=data_path,
        output_path=output_path,
    )


def start_cli():
    cli()
