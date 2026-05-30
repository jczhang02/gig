#!/usr/bin/env python3
"""Measure gig short-link download latency and throughput.

This script intentionally redacts full URLs because short-link targets may contain
private signed query strings. It can compare a short URL against its redirect target
without printing either full URL.
"""

import argparse
import os
import sqlite3
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse


CURL_WRITE_OUT = (
    "http=%{http_code} redirects=%{num_redirects} "
    "dns=%{time_namelookup} connect=%{time_connect} tls=%{time_appconnect} "
    "first_byte=%{time_starttransfer} total=%{time_total} "
    "size=%{size_download} speed=%{speed_download}\\n"
)
DEFAULT_RANGE_BYTES = 10 * 1024 * 1024


@dataclass(frozen=True)
class BenchmarkUrl:
    label: str
    url: str


@dataclass(frozen=True)
class CurlMeasurement:
    label: str
    phase: str
    run: int
    http_code: str
    redirects: int
    dns_s: float
    connect_s: float
    tls_s: float
    first_byte_s: float
    total_s: float
    size_bytes: int
    speed_bytes_s: float

    @property
    def speed_mib_s(self) -> float:
        return self.speed_bytes_s / (1024 * 1024)


def default_db_path() -> Path:
    data_home = os.environ.get("XDG_DATA_HOME")
    if data_home:
        return Path(data_home) / "gig" / "gig.db"
    return Path.home() / ".local" / "share" / "gig" / "gig.db"


def redact_url(url: str) -> str:
    parsed = urlparse(url)
    return f"{parsed.scheme}://{parsed.netloc}/…"


def fetch_urls_from_db(
    db_path: Path,
    artifact_ids: list[int],
    latest_short: int,
) -> list[BenchmarkUrl]:
    if not db_path.exists():
        raise SystemExit(f"gig database not found: {db_path}")

    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    urls: list[BenchmarkUrl] = []

    if artifact_ids:
        placeholders = ",".join("?" for _ in artifact_ids)
        rows = conn.execute(
            f"""
            select da.id, o.slug, da.local_path, da.remote_url
            from delivery_artifacts da
            left join orders o on o.id = da.order_id
            where da.id in ({placeholders}) and da.remote_url is not null
            order by da.id desc
            """,
            artifact_ids,
        )
    elif latest_short > 0:
        rows = conn.execute(
            """
            select da.id, o.slug, da.local_path, da.remote_url
            from delivery_artifacts da
            left join orders o on o.id = da.order_id
            where da.remote_url like 'https://go.jczhang.cc/%'
            order by da.id desc
            limit ?
            """,
            (latest_short,),
        )
    else:
        return []

    for row in rows:
        local_name = Path(row["local_path"]).name if row["local_path"] else "unknown"
        label = f"artifact:{row['id']} slug:{row['slug'] or 'unknown'} file:{local_name}"
        urls.append(BenchmarkUrl(label=label, url=row["remote_url"]))
    return urls


def parse_key_values(output: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for part in output.strip().split():
        if "=" not in part:
            continue
        key, value = part.split("=", 1)
        values[key] = value
    return values


def curl_output(args: list[str], timeout_s: int) -> str:
    try:
        completed = subprocess.run(
            ["curl", *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_s,
        )
    except FileNotFoundError as err:
        raise SystemExit("curl is required but was not found in PATH") from err
    except subprocess.CalledProcessError as err:
        detail = err.stderr.strip() or err.stdout.strip()
        raise RuntimeError(f"curl failed: {detail}") from err
    return completed.stdout


def redirect_location(url: str, timeout_s: int) -> str | None:
    headers = curl_output(
        [
            "-sS",
            "-D",
            "-",
            "-o",
            "/dev/null",
            "--range",
            "0-0",
            "--max-time",
            str(timeout_s),
            url,
        ],
        timeout_s + 5,
    )
    for line in headers.splitlines():
        if line.lower().startswith("location:"):
            return line.split(":", 1)[1].strip()
    return None


def measure(
    url: str,
    label: str,
    phase: str,
    run: int,
    range_bytes: int,
    timeout_s: int,
) -> CurlMeasurement:
    end_byte = range_bytes - 1
    output = curl_output(
        [
            "-sS",
            "-L",
            "-o",
            "/dev/null",
            "--range",
            f"0-{end_byte}",
            "--max-time",
            str(timeout_s),
            "-w",
            CURL_WRITE_OUT,
            url,
        ],
        timeout_s + 5,
    )
    values = parse_key_values(output)
    return CurlMeasurement(
        label=label,
        phase=phase,
        run=run,
        http_code=values.get("http", ""),
        redirects=int(values.get("redirects", "0")),
        dns_s=float(values.get("dns", "0")),
        connect_s=float(values.get("connect", "0")),
        tls_s=float(values.get("tls", "0")),
        first_byte_s=float(values.get("first_byte", "0")),
        total_s=float(values.get("total", "0")),
        size_bytes=int(float(values.get("size", "0"))),
        speed_bytes_s=float(values.get("speed", "0")),
    )


def print_measurement(measurement: CurlMeasurement) -> None:
    print(
        "\t".join(
            [
                measurement.label,
                measurement.phase,
                str(measurement.run),
                measurement.http_code,
                str(measurement.redirects),
                f"{measurement.first_byte_s:.3f}",
                f"{measurement.total_s:.3f}",
                str(measurement.size_bytes),
                f"{measurement.speed_mib_s:.2f}",
            ]
        )
    )


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--url",
        action="append",
        default=[],
        help="short or direct URL to benchmark",
    )
    parser.add_argument(
        "--artifact-id",
        action="append",
        type=int,
        default=[],
        help="delivery_artifacts.id from the local gig database",
    )
    parser.add_argument(
        "--latest-short",
        type=int,
        default=0,
        help="benchmark the N latest go.jczhang.cc artifact links",
    )
    parser.add_argument("--db", type=Path, default=default_db_path(), help="path to gig.db")
    parser.add_argument(
        "--bytes",
        type=int,
        default=DEFAULT_RANGE_BYTES,
        help="range size to download per run",
    )
    parser.add_argument("--repeat", type=int, default=3, help="number of runs per URL")
    parser.add_argument("--timeout", type=int, default=60, help="curl timeout seconds per run")
    parser.add_argument(
        "--no-direct",
        action="store_true",
        help="do not compare redirect targets",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    urls = [
        BenchmarkUrl(label=f"url:{index + 1}", url=url)
        for index, url in enumerate(args.url)
    ]
    urls.extend(fetch_urls_from_db(args.db, args.artifact_id, args.latest_short))

    if not urls:
        raise SystemExit("provide --url, --artifact-id, or --latest-short")
    if args.bytes <= 0:
        raise SystemExit("--bytes must be positive")
    if args.repeat <= 0:
        raise SystemExit("--repeat must be positive")

    print("label\tphase\trun\thttp\tredirects\tfirst_byte_s\ttotal_s\tsize_bytes\tspeed_mib_s")
    for item in urls:
        print(f"# {item.label} {redact_url(item.url)}", file=sys.stderr)
        direct_url = None if args.no_direct else redirect_location(item.url, args.timeout)
        if direct_url:
            print(f"# direct target {redact_url(direct_url)}", file=sys.stderr)
        for run in range(1, args.repeat + 1):
            print_measurement(
                measure(item.url, item.label, "short", run, args.bytes, args.timeout)
            )
            if direct_url:
                print_measurement(
                    measure(direct_url, item.label, "direct", run, args.bytes, args.timeout)
                )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
