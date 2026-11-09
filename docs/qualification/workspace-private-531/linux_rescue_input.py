"""Fixed rescue image transport primitives. Importing or invoking this module fetches nothing.

A future Nucleus proof entry must bind a clean source head, authenticated metadata, an exclusive
removable fixture, all program inputs and a single absolute deadline before calling this helper.
This module is not that entry and cannot issue a production receipt.
"""
import http.client
import os
import ssl
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import runtime_image_inputs as inputs

Refusal = inputs.Refusal
NAME = 'ubuntu-26.04.1-live-server-arm64.iso'
URL = 'https://cdimage.ubuntu.com/ubuntu/releases/26.04/release/' + NAME
SIZE = 3076861952
SHA256 = 'af78753c94b924c85c4964c670692a853f7c80299a4064e317edad95462e3168'
FINGERPRINT = '843938DF228D22F7B3742BC0D94AA3F0EFE21092'
DURATION = 1800
FIXTURE_CAP = 4 * 1024 * 1024 * 1024


def open_fixed_https(url, headers=None):
    """One fixed host and path, no redirect following and no caller-supplied headers."""
    if url != URL or headers is not None:
        raise Refusal('rescue URL or headers differ from the fixed route')
    conn = http.client.HTTPSConnection('cdimage.ubuntu.com', 443, timeout=60,
                                       context=ssl.create_default_context())
    try:
        conn.request('GET', '/ubuntu/releases/26.04/release/' + NAME,
                     headers={'Accept-Encoding': 'identity', 'User-Agent': 'effigy-qualification'})
        return conn, conn.getresponse()
    except BaseException:
        conn.close()
        raise


def fetch_fixed(destination, *, checkpoint):
    """Fixed production transport body; an external supervisor must enforce its absolute deadline.

    This body has no authority to establish the destination, metadata or receipt provenance. It
    uses the existing stream cap and exclusive create, preserving partial bytes on refusal.
    """
    if not callable(checkpoint) or not isinstance(destination, str) or not os.path.isabs(destination):
        raise Refusal('fixed fetch requires an absolute destination and a deadline checkpoint')
    if os.path.realpath(destination) != destination or os.path.basename(destination) != NAME:
        raise Refusal('destination is not the canonical fixed image path')
    checkpoint()
    return inputs.fetch_pinned_https(URL, destination, SIZE, SHA256, opener=open_fixed_https,
                                     checkpoint=checkpoint)


def main():
    raise Refusal('ordinary rescue input execution is unarmed; no acquisition entry is admitted')


if __name__ == '__main__':
    try:
        main()
    except Refusal as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        sys.exit(2)
