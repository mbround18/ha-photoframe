#!/usr/bin/env python3

import argparse
import sys

import serial


def main() -> int:
    parser = argparse.ArgumentParser(description="Read and print serial output")
    parser.add_argument("port")
    parser.add_argument("baudrate", type=int)
    args = parser.parse_args()

    with serial.Serial(args.port, args.baudrate, timeout=0.1) as port:
        try:
            while True:
                chunk = port.read(4096)
                if not chunk:
                    continue

                sys.stdout.write(chunk.decode("utf-8", errors="replace"))
                sys.stdout.flush()
        except KeyboardInterrupt:
            return 0


if __name__ == "__main__":
    raise SystemExit(main())
