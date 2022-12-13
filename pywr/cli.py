import logging
from pathlib import Path

import click

# from pywr.model import Model


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
def run(path: str):
    model = Model.from_file(Path(path))
    model.run()


def start_cli():
    cli()
