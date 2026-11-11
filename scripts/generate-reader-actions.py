#!/usr/bin/env python3
"""Validate the finite action matrix and embed identical presets in Rust/Swift."""
import argparse
import json
from pathlib import Path


def compile_policy(policy):
    actions, groups, profiles = (policy[key] for key in ('actions', 'groups', 'profiles'))
    def expand(ids, visiting=()):
        result = []
        for id in ids:
            if id.startswith('@'):
                name = id[1:]
                if name in visiting:
                    raise ValueError(f'Cyclic action group: {name}')
                result.extend(expand(groups[name], (*visiting, name)))
            else:
                if id not in actions:
                    raise ValueError(f'Unknown action: {id}')
                result.append(id)
        if len(result) != len(set(result)):
            raise ValueError(f'Duplicate actions: {ids}')
        return result
    for action in actions.values():
        if not isinstance(action['width'], int) or not 20 <= action['width'] <= 240:
            raise ValueError('Action widths must be 20–240 points')
    for ids in groups.values():
        expand(ids)
    return actions, {name: expand(ids) for name, ids in profiles.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out-dir', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    actions, profiles = compile_policy(json.loads((root / 'assets/reader-actions.json').read_text()))
    quote = json.dumps
    rust = ['// Generated from assets/reader-actions.json.\n',
            'pub(crate) fn preset(profile: &str) -> &\'static [&\'static str] { match profile {\n']
    rust += [f'{quote(name)} => &{json.dumps(ids)},\n' for name, ids in profiles.items()]
    rust += ['_ => panic!("Unknown reader action profile: {profile}"),\n}}\n',
             'pub(crate) fn action_layout(id: &str) -> (&\'static str, f32) { match id {\n']
    rust += [f'{quote(id)} => ({quote(meta["group"])}, {meta["width"]}.),\n' for id, meta in actions.items()]
    rust += ['_ => panic!("Unknown reader action: {id}"),\n}}\n']
    swift = ['// Generated from assets/reader-actions.json.\nimport Foundation\nenum ReaderActionPresets {\n',
             'static let profiles: [String: [String]] = [\n']
    swift += [f'{quote(name)}: {json.dumps(ids)},\n' for name, ids in profiles.items()]
    swift += [']\nstatic let layout: [String: (group: String, width: CGFloat)] = [\n']
    swift += [f'{quote(id)}: ({quote(meta["group"])}, {meta["width"]}),\n' for id, meta in actions.items()]
    swift += [']\n}\n']
    args.out_dir.mkdir(parents=True, exist_ok=True)
    (args.out_dir / 'reader_actions.rs').write_text(''.join(rust))
    (args.out_dir / 'ReaderActionPresets.swift').write_text(''.join(swift))

if __name__ == '__main__':
    main()
