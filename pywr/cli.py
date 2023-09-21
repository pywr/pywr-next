import logging
from pathlib import Path
from typing import Optional

import click
from . import run_model_from_string


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
@click.option("-s", "--solver", type=click.Choice(["clp", "highs"]), default="clp")
@click.option(
    "-d", "--data-path", type=click.Path(exists=True, dir_okay=True), default=None
)
@click.option("-t", "--threads", type=int, default=1)
def run(path: str, solver: str, data_path: Optional[str], threads: int):
    with open(path) as fh:
        data = fh.read()

    run_model_from_string(data, solver, data_path, None, threads)


def start_cli():
    cli()
