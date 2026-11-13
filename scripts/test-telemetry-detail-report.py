#!/usr/bin/env python3
"""Regression checks for readable shortcuts and operation/layout separation."""
import csv
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    events = []

    def event(name, **props):
        events.append({'eventName': name, 'appId': 'fixture', 'sessionId': 'session',
                       'timestamp': '2026-11-13T00:00:00Z',
                       'props': {'installation_id': 'fixture-installation', 'event_sequence': len(events), **props}})

    event('shortcut_used', action='NextMatch', key_token='g', command=True)
    event('shortcut_used', action='MoveSelected', shortcut='Cmd+Shift+M', key_token='m', command=True, shift=True)
    event('native_shortcut_used', action='copy', key_code=8, command=True, shift=True)
    event('native_shortcut_used', action='extract_text', key_code=8, key_token='i', shortcut='Cmd+Shift+I',
          key_basis='recognized_command', command=True, shift=True, schema_version=5)
    event('native_shortcut_used', action='next', key_code=121, option=True)
    event('native_shortcut_used', action='unknown', key_code=999, control=True)
    event('explorer_action', action='move', semantic='invocation', storage='ssh', input_source='drag_drop')
    event('file_operation', action='move', semantic='outcome', storage='ssh', result='failed')
    event('foreground_segment', layout_mode='three_column', storage='local', duration_ms=1000)
    event('foreground_segment', layout_mode='three_column', storage='local', duration_ms=2000)
    event('foreground_segment', layout_mode='workspace_reader', storage='ssh', duration_ms=0)
    event('foreground_segment', duration_ms=500)
    event('resource_sample', resident_bytes=100)
    events.append(events[0])
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        source = root / 'events.json'
        source.write_text(json.dumps(events), encoding='utf-8')
        subprocess.run([sys.executable, str(Path(__file__).with_name('telemetry-detail-report.py')),
                        str(source), '--output-dir', str(root / 'report')], check=True)

        def rows(name):
            with (root / 'report' / name).open(encoding='utf-8-sig', newline='') as stream:
                return list(csv.DictReader(stream))

        shortcuts = {row['shortcut']: row for row in rows('shortcuts.csv')}
        assert set(shortcuts) == {'Cmd+G', 'Cmd+Shift+M', 'Cmd+Shift+C', 'Cmd+Shift+I', 'Option+PAGEDOWN', 'Ctrl+KEYCODE:999'}
        assert shortcuts['Cmd+G']['uses'] == '1', 'Duplicate exports must not inflate counts'
        assert shortcuts['Cmd+Shift+C']['key_basis'] == 'macos_virtual_key_us'
        assert shortcuts['Cmd+Shift+I']['key_basis'] == 'recognized_command'
        actions = rows('actions.csv')
        assert not any(row['event'] == 'resource_sample' for row in actions)
        assert any(row['action'] == 'move' and row['semantic'] == 'invocation' and row['source'] == 'drag_drop' for row in actions)
        assert any(row['action'] == 'move' and row['semantic'] == 'outcome' and row['result'] == 'failed' for row in actions)
        layouts = {row['layout_mode']: row for row in rows('layouts.csv')}
        assert layouts['three_column']['unique_installations'] == '1'
        assert layouts['three_column']['foreground_seconds'] == '3.0'
        assert layouts['legacy_unknown']['foreground_seconds'] == '0.5'
        assert 'workspace_reader' not in layouts, 'Zero duration is not observed layout usage'
    print('Telemetry detail report: passed')


if __name__ == '__main__':
    main()
