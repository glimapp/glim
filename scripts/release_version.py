"""Stable product versions use three components plus an optional hotfix number."""
import re
import sys


def parse_version(version):
    if not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\.([1-9]\d*))?', version):
        raise ValueError('Expected MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH.HOTFIX')
    parts = tuple(map(int, version.split('.')))
    return parts + (0,) * (4 - len(parts))


def cargo_version(version):
    parse_version(version)
    parts = version.split('.')
    return '.'.join(parts[:3]) + (f'+hotfix.{parts[3]}' if len(parts) == 4 else '')


def validate_tag(tag):
    if not tag.startswith('v'):
        raise ValueError('Stable release tags must start with v')
    parse_version(tag[1:])
    return tag[1:]


if __name__ == '__main__':
    print(validate_tag(sys.argv[1]))
