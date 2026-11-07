#!/usr/bin/env python3
"""Validate the complete release, aggregate addon catalogs, and generate checksums."""
import json
from pathlib import Path
import sys
from package import RELEASE_TARGETS, asset_name
from addon_release import manifests, archive_name, index_name, digest
import tomllib


def generate(directory, tag, root=Path('.')):
    definitions = {m['id']: m for _, m in manifests(root)}
    editor_version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
    expected = {asset_name(tag, target) for target in RELEASE_TARGETS}
    expected |= {index_name(target) for target in RELEASE_TARGETS}
    expected |= {archive_name(tag, target, addon) for target in RELEASE_TARGETS for addon in definitions}
    actual = {p.name for p in directory.iterdir()}
    if actual - {'SHA256SUMS', 'addons.json'} != expected:
        raise RuntimeError(f'Unexpected or missing release assets: {sorted(actual)}; expected {sorted(expected)}')
    for name in expected:
        path = directory / name
        if not path.is_file() or path.is_symlink() or path.stat().st_size == 0:
            raise RuntimeError(f'Invalid release asset: {name}')
    packages = []
    seen = set()
    for target in RELEASE_TARGETS:
        catalog = json.loads((directory / index_name(target)).read_text())
        if catalog['schema'] != 1 or catalog['release'] != tag or catalog['editor_version'] != editor_version:
            raise RuntimeError('Release catalog identity mismatch')
        for package in catalog['packages']:
            key = (package['id'], package['target'])
            if key in seen or package['id'] not in definitions or package['target'] != target:
                raise RuntimeError('Duplicate or unexpected addon package')
            seen.add(key)
            definition = definitions[package['id']]
            name = archive_name(tag, target, package['id'])
            path = directory / name
            if package['archive'] != name or package['version'] != definition['version'] or package['protocol'] != definition['protocol'] or package['sha256'] != digest(path) or package['size'] != path.stat().st_size:
                raise RuntimeError('Addon package metadata/checksum mismatch')
            packages.append(package)
    if seen != {(addon, target) for addon in definitions for target in RELEASE_TARGETS}:
        raise RuntimeError('Incomplete addon catalog')
    aggregate = {'schema': 1, 'release': tag, 'editor_version': editor_version,
                 'packages': sorted(packages, key=lambda p: (p['id'], p['target']))}
    (directory / 'addons.json').write_text(json.dumps(aggregate, indent=2) + '\n')
    records = [f'{digest(directory / name)}  {name}\n' for name in sorted(expected | {'addons.json'})]
    (directory / 'SHA256SUMS').write_text(''.join(records), encoding='utf-8')


if __name__ == '__main__':
    generate(Path(sys.argv[1]), sys.argv[2])
