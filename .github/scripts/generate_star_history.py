#!/usr/bin/env python3
"""Generate a self-hosted cumulative GitHub stars chart and sync README star visibility."""

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
import re
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path


def fetch_stars(repository: str, token: str) -> list[dt.date]:
    url = f"https://api.github.com/repos/{repository}/stargazers?per_page=100"
    dates: list[dt.date] = []
    while url:
        request = urllib.request.Request(
            url,
            headers={
                "Accept": "application/vnd.github.star+json",
                "Authorization": f"Bearer {token}",
                "User-Agent": "pulsar-star-history",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urllib.request.urlopen(request) as response:
            for star in json.load(response):
                dates.append(dt.datetime.fromisoformat(star["starred_at"].replace("Z", "+00:00")).date())
            links = response.headers.get("Link", "")
        url = ""
        for link in links.split(","):
            if 'rel="next"' in link:
                url = link[link.index("<") + 1 : link.index(">")]
                break
    return dates


def week_start(day: dt.date) -> dt.date:
    """Return the Monday containing ``day``."""
    return day - dt.timedelta(days=day.weekday())


def render_svg(repository: str, dates: list[dt.date], today: dt.date | None = None) -> str:
    today = today or dt.date.today()
    sorted_dates = sorted(dates)
    current_week = week_start(today)
    weeks = [current_week - dt.timedelta(weeks=week) for week in reversed(range(26))]
    values = [sum(day <= min(week + dt.timedelta(days=6), today) for day in sorted_dates) for week in weeks]
    weekly = [
        sum(week <= day <= min(week + dt.timedelta(days=6), today) for day in sorted_dates)
        for week in weeks
    ]

    width, height = 800, 420
    left, right, top, bottom = 68, 24, 78, 64
    plot_w, plot_h = width - left - right, height - top - bottom
    latest = values[-1]
    magnitude = 10 ** max(0, len(str(max(latest, 1))) - 2)
    step = max(magnitude, math.ceil(latest / 4 / magnitude) * magnitude)
    grid_max = max(step * 4, 4)

    def x(index: int) -> float:
        return left + index * plot_w / (len(weeks) - 1)

    def y(value: int) -> float:
        return top + (1 - value / grid_max) * plot_h

    y_ticks = []
    for index in range(5):
        value = step * index
        yy = y(value)
        label = f"{value / 1000:g}k" if value >= 1000 else str(value)
        y_ticks.append(
            f'<line x1="{left}" y1="{yy:.1f}" x2="{width - right}" y2="{yy:.1f}" class="grid"/>'
            f'<text x="{left - 12}" y="{yy + 5:.1f}" text-anchor="end">{label}</text>'
        )

    points = " ".join(f"{x(index):.1f},{y(value):.1f}" for index, value in enumerate(values))
    area = f"{left},{top + plot_h:.1f} {points} {width - right},{top + plot_h:.1f}"
    x_ticks = []
    dots = []
    for index, (week, value, gain) in enumerate(zip(weeks, values, weekly)):
        if index % 4 == 0 or index == len(weeks) - 1:
            x_ticks.append(
                f'<text x="{x(index):.1f}" y="{height - 30}" text-anchor="middle">{week:%b %-d}</text>'
            )
        current = " current" if week == current_week else ""
        radius = 5 if current else 3
        dots.append(
            f'<circle class="point{current}" cx="{x(index):.1f}" cy="{y(value):.1f}" r="{radius}">'
            f'<title>{week:%b %-d}: {value:,} total stars (+{gain:,} that week)</title></circle>'
        )

    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">
<title id="title">{repository} cumulative GitHub stars over time</title>
<desc id="desc">Cumulative GitHub stars sampled weekly over the last 26 weeks, ending at {latest:,} stars.</desc>
<style>
  :root {{ color-scheme: light dark; }}
  .bg {{ fill: #fff; }} text {{ fill: #57606a; font: 13px -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; }}
  .grid {{ stroke: #d8dee4; stroke-width: 1; }}
  .area {{ fill: #2781f6; fill-opacity: .14; }} .line {{ fill: none; stroke: #2781f6; stroke-width: 3; stroke-linejoin: round; stroke-linecap: round; }}
  .point {{ fill: #fff; stroke: #2781f6; stroke-width: 2; }} .point.current {{ fill: #2781f6; stroke: #fff; stroke-width: 2; }}
  .heading {{ fill: #24292f; font-size: 17px; font-weight: 600; }} .metric {{ fill: #24292f; font-size: 18px; font-weight: 600; }}
  @media (prefers-color-scheme: dark) {{ .bg {{ fill: #0d1117; }} text {{ fill: #8b949e; }} .grid {{ stroke: #30363d; }} .area {{ fill: #58a6ff; fill-opacity: .16; }} .line {{ stroke: #58a6ff; }} .point {{ fill: #0d1117; stroke: #58a6ff; }} .point.current {{ fill: #58a6ff; stroke: #0d1117; }} .heading,.metric {{ fill: #f0f6fc; }} }}
</style>
<rect class="bg" width="100%" height="100%" rx="6"/>
<text class="heading" x="{left}" y="28">GitHub stars over time</text>
<text x="{left}" y="51">Cumulative stars · weekly sampling · last 26 weeks</text>
<text class="metric" x="{width - right}" y="28" text-anchor="end">{latest:,} stars</text>
<text x="{width - right}" y="51" text-anchor="end">+{weekly[-1]:,} this week so far</text>
{''.join(y_ticks)}
<polygon class="area" points="{area}"/>
<polyline class="line" points="{points}"/>
{''.join(dots)}
{''.join(x_ticks)}
</svg>
'''


def _replace_marker_block(text: str, name: str, inner: str) -> str:
    pattern = re.compile(rf"(<!-- {re.escape(name)}:start -->).*?(<!-- {re.escape(name)}:end -->)", re.DOTALL)
    return pattern.sub(lambda m: f"{m.group(1)}{inner}{m.group(2)}", text, count=1)


def sync_readme(readme_text: str, repository: str, star_count: int) -> str:
    """Show README star badge and Star History chart only once star_count >= 1."""
    if star_count >= 1:
        badge = (
            f'\n  <a href="https://github.com/{repository}/stargazers">'
            f'<img src="https://img.shields.io/github/stars/{repository}?style=flat-square&amp;logo=github" alt="GitHub Stars"></a>'
        )
        section = (
            f"\n\n---\n\n<div align=\"center\">\n\n## Star History\n\n"
            f'<a href="https://github.com/{repository}/stargazers">\n'
            f'  <img src="docs/images/star-history.svg" alt="Pulsar cumulative GitHub stars over time" width="800">\n'
            f"</a>\n\n</div>\n"
        )
    else:
        badge = ""
        section = ""

    updated = _replace_marker_block(readme_text, "stars-badge", badge)
    return _replace_marker_block(updated, "star-history", section)


def self_test() -> None:
    today = dt.date(2026, 11, 27)
    empty_svg = render_svg("aphelion31/pulsar", [], today=today)
    ET.fromstring(empty_svg)
    assert "0 stars" in empty_svg
    assert "+0 this week so far" in empty_svg

    few_svg = render_svg("aphelion31/pulsar", [today - dt.timedelta(days=14), today - dt.timedelta(days=1), today], today=today)
    ET.fromstring(few_svg)
    assert "3 stars" in few_svg
    assert "+2 this week so far" in few_svg

    many = [today - dt.timedelta(days=i % 120) for i in range(1500)]
    many_svg = render_svg("aphelion31/pulsar", many, today=today)
    ET.fromstring(many_svg)
    assert "1,500 stars" in many_svg

    sample = (
        "<p><!-- stars-badge:start --><!-- stars-badge:end --></p>\n"
        "<!-- star-history:start --><!-- star-history:end -->\n"
    )
    hidden = sync_readme(sample, "aphelion31/pulsar", 0)
    assert "GitHub Stars" not in hidden and "Star History" not in hidden
    shown = sync_readme(hidden, "aphelion31/pulsar", 5)
    assert "GitHub Stars" in shown and "Star History" in shown
    assert sync_readme(shown, "aphelion31/pulsar", 5) == shown
    assert sync_readme(shown, "aphelion31/pulsar", 0) == hidden


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", default="aphelion31/pulsar")
    parser.add_argument("--output", type=Path, default=Path("docs/images/star-history.svg"))
    parser.add_argument("--readme", type=Path, default=Path("README.md"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("star-history self-test passed")
        return
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if not token:
        raise SystemExit("GITHUB_TOKEN or GH_TOKEN is required")
    dates = fetch_stars(args.repo, token)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(render_svg(args.repo, dates))
    if args.readme.exists():
        args.readme.write_text(sync_readme(args.readme.read_text(), args.repo, len(dates)))


if __name__ == "__main__":
    main()
