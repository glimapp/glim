#!/usr/bin/env python3
"""Rank format reach from a JSON/JSONL/CSV event export. No API credentials needed."""
import argparse
import csv
import gzip
import json
import math
from collections import defaultdict
from datetime import datetime, timezone, timedelta
from pathlib import Path


def records(path):
    suffixes = path.suffixes
    compressed = suffixes and suffixes[-1].lower() == '.gz'
    suffix = suffixes[-2].lower() if compressed and len(suffixes) > 1 else path.suffix.lower()
    opener = gzip.open if compressed else open
    if suffix == '.csv':
        with opener(path, 'rt', newline='', encoding='utf-8-sig') as stream:
            yield from csv.DictReader(stream)
        return
    if suffix in {'.jsonl', '.ndjson'}:
        with opener(path, 'rt', encoding='utf-8-sig') as stream:
            for line in stream:
                if line.strip():
                    yield json.loads(line)
        return
    with opener(path, 'rt', encoding='utf-8-sig') as stream:
        source = stream.read()
    try:
        data = json.loads(source)
    except json.JSONDecodeError:
        for line in source.splitlines():
            if line.strip():
                yield json.loads(line)
        return
    if isinstance(data, dict):
        data = data.get('events', data.get('data', [data]))
    if not isinstance(data, list):
        raise ValueError('Expected event array or {events: [...]} / {data: [...]}')
    yield from data


def object_value(value):
    return json.loads(value) if isinstance(value, str) and value else value or {}


def add_filters(parser):
    parser.add_argument('--app-id', help='Keep one app; use the production ID to omit DEBUG and other products')
    parser.add_argument('--exclude-installation-id', action='append', default=[])
    parser.add_argument('--exclude-installations-file', type=Path, help='Local UTF-8 list of IDs, one per line; # comments allowed')


def filtered_records(args):
    excluded = set(args.exclude_installation_id)
    if args.exclude_installations_file:
        excluded.update(line.split('#', 1)[0].strip() for line in args.exclude_installations_file.read_text(encoding='utf-8-sig').splitlines())
    excluded.discard('')
    seen = set()
    for event in records(args.export):
        app = event.get('app_id', event.get('appId', ''))
        if args.app_id and app != args.app_id:
            continue
        props = {**object_value(event.get('string_props')), **object_value(event.get('numeric_props')), **object_value(event.get('props', event.get('properties')))}
        identity = props.get('installation_id', '')
        if identity and identity in excluded:
            continue
        session = str(event.get('sessionId', event.get('session_id', '')))
        sequence = props.get('event_sequence')
        if sequence is not None and session:
            key = (app, identity, session, str(sequence))
            if key in seen:
                continue
            seen.add(key)
        system = object_value(event.get('systemProps', event.get('system_props')))
        event['systemProps'] = {'osVersion': event.get('os_version', ''), 'appVersion': event.get('app_version', ''), **system}
        event['props'] = props
        event['sessionId'] = session
        event['eventName'] = event.get('eventName', event.get('event_name', event.get('name', 'unknown')))
        event['appId'] = app
        yield event


def local_day(event):
    timestamp = event.get('timestamp')
    if not timestamp:
        return ''
    at = datetime.fromisoformat(timestamp.replace('Z', '+00:00'))
    if at.tzinfo is None:
        at = at.replace(tzinfo=timezone.utc)
    return at.astimezone(timezone(timedelta(hours=8))).date().isoformat()


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)] if values else ''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('export', type=Path)
    parser.add_argument('--output', type=Path, help='CSV output; defaults to stdout')
    add_filters(parser)
    args = parser.parse_args()
    formats = defaultdict(lambda: dict(opens=0, failed=0, people=set(), successful=set(), sessions=set(), days=defaultdict(set), latency=[], native_failed=0, fallbacks=0, dwell_ms=0, unidentified=0))
    all_people = set()
    for event in filtered_records(args):
        props = event['props']
        name = event['eventName']
        session = event['sessionId']
        identity = props.get('installation_id')
        if identity:
            all_people.add(identity)
        extension = props.get('file_extension')
        if not extension:
            continue
        row = formats[extension]
        if name == 'file_open':
            row['opens'] += 1
            row['failed'] += props.get('result') == 'failed'
            if identity:
                row['people'].add(identity)
                if props.get('result') == 'success':
                    row['successful'].add(identity)
                    day = local_day(event)
                    if day:
                        row['days'][identity].add(day)
            else:
                row['unidentified'] += 1
            if session:
                row['sessions'].add(session)
            duration = props.get('duration_ms')
            if isinstance(duration, (float, int)) and duration >= 0:
                row['latency'].append(duration)
        elif name == 'native_load' and props.get('result') == 'failed':
            row['native_failed'] += 1
        elif name == 'reader_fallback':
            row['fallbacks'] += 1
        elif name == 'foreground_segment':
            row['dwell_ms'] += props.get('duration_ms', 0)
    import sys
    output = args.output.open('w', newline='', encoding='utf-8-sig') if args.output else sys.stdout
    try:
        writer = csv.writer(output)
        writer.writerow(['format', 'unique_installations', 'reach_percent', 'open_attempts', 'sessions', 'open_failed', 'open_failure_percent', 'open_p50_ms', 'open_p95_ms', 'native_load_failed', 'foreground_seconds', 'opens_without_identity', 'successful_installations', 'repeat_open_installations_utc8', 'source_fallbacks'])
        for extension, row in sorted(formats.items(), key=lambda item: (-len(item[1]['people']), -item[1]['opens'], item[0])):
            if not row['opens']:
                continue
            writer.writerow([extension, len(row['people']), round(100 * len(row['people']) / len(all_people), 2) if all_people else '', row['opens'], len(row['sessions']), row['failed'], round(100 * row['failed'] / row['opens'], 2), percentile(row['latency'], .5), percentile(row['latency'], .95), row['native_failed'], round(row['dwell_ms'] / 1000, 1), row['unidentified'], len(row['successful']), sum(len(days) > 1 for days in row['days'].values()), row['fallbacks']])
    finally:
        if args.output:
            output.close()


if __name__ == '__main__':
    main()
