"""Read-only Desktop CLI checks for the unsealed preparation and session entries."""
import os
from pathlib import Path
import shutil
import subprocess

from common import artifact, digest, exclusive, require


def resolve_cli(item=None):
    selected = shutil.which('docker')
    require(selected is not None, 'desktop-cli-unavailable: docker is absent from PATH')
    link = Path(selected)
    target = link.resolve(strict=True)
    observed = digest(target)
    if item is not None:
        require(Path(item['path']).resolve(strict=True) == target and item['sha256'] == observed,
                'desktop-cli-pin-mismatch: PATH differs from engine_binary')
    artifact(dict(path=str(target), sha256=observed))
    return dict(path=str(link), link_target=os.readlink(link) if link.is_symlink() else None,
                target=str(target), sha256=observed)


def preflight(env, evidence, *, item=None):
    """Write only the fresh owned config; never read or modify Desktop's config."""
    cli = resolve_cli(item)
    plugins = Path(cli['target']).parent.parent / 'cli-plugins'
    require(plugins.is_dir() and (plugins / 'docker-compose').is_file(),
            'desktop-compose-unavailable: CLI bundle has no Compose plugin directory')
    config = Path(env['DOCKER_CONFIG'])
    config.mkdir(mode=0o700, exist_ok=True)
    require(config.resolve(strict=True) == config and config.stat().st_uid == os.geteuid()
            and not list(config.iterdir()), 'desktop-config-not-fresh: private Docker config must be empty and owned')
    exclusive(config / 'config.json', dict(cliPluginsExtraDirs=[str(plugins)]))
    receipt = dict(proof=False, cli=cli, compose=dict(path=str(plugins / 'docker-compose'),
                   sha256=digest((plugins / 'docker-compose').resolve(strict=True))),
                   home=env['HOME'], docker_config=str(config), checks=[])
    try:
        for args, cause in [(['version'], 'desktop-version-preflight'),
                            (['compose', 'version'], 'desktop-compose-preflight')]:
            argv = [cli['target'], *args]
            try:
                child = subprocess.run(argv, env=env, capture_output=True, timeout=30)
            except (OSError, subprocess.TimeoutExpired) as exc:
                receipt['checks'].append(dict(argv=argv, cause=cause, error=str(exc)))
                raise RuntimeError(cause + ': command unavailable or exceeded 30 seconds') from exc
            receipt['checks'].append(dict(argv=argv, exit=child.returncode,
                    stdout=child.stdout.decode(errors='replace'), stderr=child.stderr.decode(errors='replace')))
            require(child.returncode == 0, cause + ': refused under the exact private HOME/config')
        return receipt
    finally:
        exclusive(evidence, receipt)
