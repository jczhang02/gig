# gig Short-Link Download Speed Notes

## Scope

Investigate TODO 1 from `2026-05-29-gig-remaining-work-todos.md`: client downloads through `go.jczhang.cc` short links feel slow.

## Measurement harness

Added a repeatable local harness:

```bash
scripts/benchmark-short-link-download.py --artifact-id 10 --bytes 10485760 --repeat 3
```

The script reads local `delivery_artifacts` rows, redacts full short and signed URLs, measures a byte-range download with `curl`, and compares the short URL with its redirect target.

## Baseline evidence

Measured against recent short-linked artifact `artifact:10` (`bert-crf-manuscript-20260528.zip`) on 2026-05-29.

| phase | redirects | first byte (s) | total for 10 MiB (s) | throughput (MiB/s) |
| --- | ---: | ---: | ---: | ---: |
| short run 1 | 1 | 1.012 | 2.928 | 3.42 |
| direct run 1 | 0 | 0.783 | 2.626 | 3.81 |
| short run 2 | 1 | 2.938 | 4.660 | 2.15 |
| direct run 2 | 0 | 0.750 | 2.535 | 3.94 |
| short run 3 | 1 | 0.919 | 2.696 | 3.71 |
| direct run 3 | 0 | 0.740 | 2.535 | 3.94 |

Additional redirect-only probes showed the short-link service returns HTTP 302 without proxying file bytes. Normal redirect latency was about 0.14–0.17 s, with occasional spikes caused by DNS/TLS/network setup.

## Finding

The short-link service is already using redirect semantics, so it is not the sustained-throughput bottleneck. The main download path is the presigned Alibaba Cloud OSS Hong Kong endpoint. In this environment, direct OSS downloads for a 10 MiB range were about 3.8–3.9 MiB/s, and the short link adds one extra connection plus occasional redirect latency spikes.

## Implemented mitigation

`gig` now supports an optional S3 `download_endpoint` setting. Uploads still use the regional `endpoint`, but generated presigned download URLs can use a faster compatible endpoint, such as Alibaba Cloud OSS transfer acceleration, after that feature is enabled on the bucket.

Example:

```toml
[delivery.s3.aliyun-hk]
endpoint = "https://s3.oss-cn-hongkong.aliyuncs.com"
download_endpoint = "https://oss-accelerate.aliyuncs.com"
```

## Validation evidence

After enabling Alibaba Cloud OSS transfer acceleration, a 5 MiB test object was uploaded twice through the integration harness: once with the regular Hong Kong endpoint and once with `download_endpoint = "https://oss-accelerate.aliyuncs.com"`. Measurements were taken from the current workstation on 2026-05-30.

| target | redirects | first byte (s) | total for 5 MiB (s) | throughput (MiB/s) |
| --- | ---: | ---: | ---: | ---: |
| regular run 1 | 0 | 0.809 | 2.233 | 2.24 |
| accelerated run 1 | 0 | 0.287 | 0.752 | 6.65 |
| regular run 2 | 0 | 0.782 | 2.142 | 2.33 |
| accelerated run 2 | 0 | 0.118 | 0.580 | 8.63 |
| regular run 3 | 0 | 0.802 | 2.191 | 2.28 |
| accelerated run 3 | 0 | 0.134 | 0.595 | 8.41 |

A newly-created short link targeting the accelerated presigned URL also resolved correctly to the accelerated host. The short-link hop still adds one Cloudflare redirect and connection setup, but the sustained object download is now served by the accelerated OSS endpoint.

| target | redirects | first byte (s) | total for 5 MiB (s) | throughput (MiB/s) |
| --- | ---: | ---: | ---: | ---: |
| direct accelerated run 1 | 0 | 0.160 | 0.627 | 7.98 |
| short accelerated run 1 | 1 | 1.216 | 1.688 | 2.96 |
| direct accelerated run 2 | 0 | 0.117 | 0.577 | 8.66 |
| short accelerated run 2 | 1 | 0.588 | 1.086 | 4.61 |
| direct accelerated run 3 | 0 | 0.214 | 0.679 | 7.37 |
| short accelerated run 3 | 1 | 0.604 | 1.126 | 4.44 |

After changing the endpoint, rerun the benchmark harness against a newly generated short link and compare `short` versus `direct` rows before using the link for client delivery. Existing short links keep pointing at their originally signed URL and must be regenerated to use the accelerated endpoint.
