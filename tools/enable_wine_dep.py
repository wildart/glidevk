#!/usr/bin/env python3
"""Enable DEP on a copied 32-bit Windows game installation. Close the game first."""
import argparse
from pathlib import Path
import struct
import tempfile


def enable_dep(path):
    original = path.read_bytes()
    if len(original) < 64 or original[:2] != b"MZ":
        raise ValueError(f"{path}: not a PE executable")
    pe = struct.unpack_from("<I", original, 60)[0]
    if (
        pe + 96 > len(original)
        or original[pe:pe + 4] != b"PE\0\0"
        or struct.unpack_from("<H", original, pe + 4)[0] != 0x14C
        or struct.unpack_from("<H", original, pe + 20)[0] < 72
        or pe + 24 + struct.unpack_from("<H", original, pe + 20)[0] > len(original)
        or struct.unpack_from("<H", original, pe + 24)[0] != 0x10B
    ):
        raise ValueError(f"{path}: expected a 32-bit x86 PE executable")
    offset = pe + 24 + 70
    flags = struct.unpack_from("<H", original, offset)[0]
    if flags & 0x100:
        return False
    backup = path.parent / "before-dep" / path.name
    backup.parent.mkdir(exist_ok=True)
    with backup.open("xb") as output:
        output.write(original)
    # Change only the NX_COMPAT bit; preserve the remaining file and its permissions.
    with path.open("r+b") as output:
        output.seek(offset + 1)
        output.write(bytes([original[offset + 1] | 1]))
    return True


def self_test():
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "app.exe"
        original = bytearray(512)
        original[:2] = b"MZ"
        struct.pack_into("<I", original, 60, 128)
        original[128:132] = b"PE\0\0"
        struct.pack_into("<H", original, 132, 0x14C)
        struct.pack_into("<H", original, 148, 224)
        struct.pack_into("<H", original, 152, 0x10B)
        struct.pack_into("<H", original, 222, 0x40)
        path.write_bytes(original)
        assert enable_dep(path)
        expected = original.copy()
        expected[223] = 1
        assert path.read_bytes() == expected
        assert (path.parent / "before-dep" / path.name).read_bytes() == original
        assert not enable_dep(path)
        path.write_bytes(original)
        try:
            enable_dep(path)
        except FileExistsError:
            pass
        else:
            raise AssertionError("Overwrote an existing backup")
        assert path.read_bytes() == original
        path.write_bytes(b"MZ")
        try:
            enable_dep(path)
        except ValueError:
            pass
        else:
            raise AssertionError("Accepted a truncated executable")
        assert path.read_bytes() == b"MZ"
        assert (path.parent / "before-dep" / path.name).read_bytes() == original
    print("DEP patch checks passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game_directory", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.game_directory:
        files = sorted(
            path for path in args.game_directory.iterdir()
            if path.is_file() and path.suffix.lower() in {".exe", ".dll"}
        )
        if not files:
            raise FileNotFoundError(f"{args.game_directory}: no .exe or .dll files found")
        for path in files:
            if enable_dep(path):
                print(f"Enabled DEP: {path.name}")
    else:
        parser.error("provide a game directory or --self-test")
