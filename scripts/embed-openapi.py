#!/usr/bin/env python3
"""Embed the offline API reference in each native preview executable."""
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
html = (root / 'assets/openapi/viewer.html').read_text()
delimiter = '####'
if f'"""{delimiter}' in html or f'\\{delimiter}(' in html:
    raise ValueError('OpenAPI HTML conflicts with Swift raw-string delimiters')
target = Path(sys.argv[1]) / 'OpenAPIAssets.swift'
target.write_text(f'enum OpenAPIAssets {{ static let html = {delimiter}"""\n{html}\n"""{delimiter} }}\n')
