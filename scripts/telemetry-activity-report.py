#!/usr/bin/env python3
"""Count observed active installations with usage statistics enabled, not all users."""
import argparse
import csv
import json
import runpy
from collections import defaultdict
from datetime import date, timedelta
from pathlib import Path

helpers = runpy.run_path(str(Path(__file__).with_name('telemetry-format-report.py')))


def write_report(args):
    daily = defaultdict(lambda: defaultdict(set))
    observed_days = defaultdict(lambda: defaultdict(set))
    export_days = defaultdict(set)
    unidentified = defaultdict(int)
    legacy = defaultdict(int)
    for event in helpers['filtered_records'](args):
        day_text = helpers['local_day'](event)
        if not day_text:
            continue
        day = date.fromisoformat(day_text)
        if args.as_of and day > args.as_of:
            continue
        app = event['appId']
        export_days[app].add(day)
        props = event['props']
        # Startup, background traffic and error-only sessions do not establish
        # foreground use. Include home/settings/drafts, not only file opens.
        if event['eventName'] != 'foreground_segment':
            continue
        duration = props.get('duration_ms', 0)
        if not isinstance(duration, (int, float)) or duration <= 0:
            continue
        allowed = props.get('usage_statistics')
        if allowed is not None and allowed not in (True, 1, 'true', '1'):
            continue
        if props.get('schema_version', 0) >= 4 and allowed not in (True, 1, 'true', '1'):
            continue
        identity = props.get('installation_id')
        if not identity:
            unidentified[app] += 1
            continue
        if allowed is None:
            legacy[app] += 1
        daily[app][day].add(identity)
        observed_days[app][identity].add(day)

    args.output_dir.mkdir(parents=True, exist_ok=True)
    summaries = []
    with (args.output_dir / 'active-installations.csv').open('w', newline='', encoding='utf-8-sig') as output:
        writer = csv.writer(output)
        writer.writerow(['app_id', 'day_utc8', 'observed_active_installations_usage_enabled'])
        for app, days in sorted(export_days.items()):
            as_of = args.as_of or max(days)
            for day, identities in sorted(daily[app].items()):
                writer.writerow([app, day.isoformat(), len(identities)])
            counts = {}
            for length in (1, 7, 30):
                start = as_of - timedelta(days=length - 1)
                identities = set()
                for day, active in daily[app].items():
                    if start <= day <= as_of:
                        identities.update(active)
                counts[str(length)] = len(identities)
            summaries.append({
                'app_id': app,
                'as_of_day_utc8': as_of.isoformat(),
                'export_first_day_utc8': min(days).isoformat(),
                'export_last_day_utc8': max(days).isoformat(),
                'observed_active_installations': counts,
                'windows_extending_beyond_export_dates': [
                    length for length in (1, 7, 30)
                    if min(days) > as_of - timedelta(days=length - 1) or max(days) < as_of
                ],
                'foreground_records_without_identity': unidentified[app],
                'legacy_foreground_records_without_consent_field': legacy[app],
            })

    with (args.output_dir / 'observed-return.csv').open('w', newline='', encoding='utf-8-sig') as output:
        writer = csv.writer(output)
        writer.writerow(['app_id', 'first_observed_day_utc8', 'days_later', 'observed_cohort_installations',
                         'observed_returning_installations', 'unobserved_installations_status_unknown',
                         'observed_return_percent'])
        for app, installations in sorted(observed_days.items()):
            as_of = args.as_of or max(export_days[app])
            cohorts = defaultdict(set)
            for identity, days in installations.items():
                cohorts[min(days)].add(identity)
            for first, cohort in sorted(cohorts.items()):
                for offset in (1, 7, 30):
                    target = first + timedelta(days=offset)
                    if target > as_of:
                        continue
                    returning = len(cohort & daily[app].get(target, set()))
                    writer.writerow([app, first.isoformat(), offset, len(cohort), returning,
                                     len(cohort) - returning, round(100 * returning / len(cohort), 2)])

    summary = {
        'metric': 'Active installations with usage statistics enabled',
        'metric_zh': '开启使用统计的活跃安装数',
        'definition': 'Distinct random installation IDs with a positive foreground segment, grouped by the segment timestamp in UTC+8.',
        'scope': 'Observed exported data only. A 7/30-day count covers that many calendar dates ending on as_of; it is not a sum of daily counts.',
        'missing_status': 'unknown',
        'limitations': [
            'Opt-outs, undelivered events and missing export data are unobserved, not churn.',
            'Error reports and diagnostic-only installations do not count as activity.',
            'Foreground occupancy can include idle time; it does not prove attention.',
            'Installations are not people: one person may use multiple devices or reset application data.',
            'First observed date in this export is not the installation date. Return rates describe observed return, not confirmed retention of all users.',
            'No identifier is invented for records without installation_id. Historical foreground records before schema 4 used the combined telemetry switch.',
        ],
        'applications': summaries,
    }
    (args.output_dir / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    for item in summaries:
        counts = item['observed_active_installations']
        print(f"{item['app_id']} · {item['as_of_day_utc8']} UTC+8 · usage-enabled active installations: "
              f"day={counts['1']}, 7 days={counts['7']}, 30 days={counts['30']}")
        if item['windows_extending_beyond_export_dates']:
            print(f"Partial windows: {item['windows_extending_beyond_export_dates']}; "
                  f"exported dates span {item['export_first_day_utc8']} to {item['export_last_day_utc8']}.")
    print('Unobserved installations have unknown status; no churn count is inferred.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('export', type=Path)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--as-of', type=date.fromisoformat, help='Final calendar date in UTC+8; default is the latest exported date per app')
    helpers['add_filters'](parser)
    write_report(parser.parse_args())


if __name__ == '__main__':
    main()
