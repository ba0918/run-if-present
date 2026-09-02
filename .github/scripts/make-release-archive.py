#!/usr/bin/env python3
import gzip
import io
import os
import pathlib
import sys
import tarfile

binary, version, target, output = sys.argv[1:]
epoch = int(os.environ["SOURCE_DATE_EPOCH"])
root = f"run-if-present-v{version}-{target}"
output_path = pathlib.Path(output)
output_path.mkdir(parents=True, exist_ok=True)
archive = output_path / f"{root}.tar.gz"
files = [
    (pathlib.Path(binary), "run-if-present", 0o755),
    (pathlib.Path("README.md"), "README.md", 0o644),
    (pathlib.Path("LICENSE-MIT"), "LICENSE-MIT", 0o644),
    (pathlib.Path("LICENSE-APACHE"), "LICENSE-APACHE", 0o644),
]

with archive.open("wb") as raw:
    with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.GNU_FORMAT) as tar:
            directory = tarfile.TarInfo(root)
            directory.type = tarfile.DIRTYPE
            directory.mode = 0o755
            directory.mtime = epoch
            directory.uid = directory.gid = 0
            directory.uname = directory.gname = ""
            tar.addfile(directory)
            for source, name, mode in files:
                data = source.read_bytes()
                entry = tarfile.TarInfo(f"{root}/{name}")
                entry.size = len(data)
                entry.mode = mode
                entry.mtime = epoch
                entry.uid = entry.gid = 0
                entry.uname = entry.gname = ""
                tar.addfile(entry, io.BytesIO(data))

print(archive)
