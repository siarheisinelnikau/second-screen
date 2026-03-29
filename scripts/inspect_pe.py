"""Inspect a packaged EXE without loading or executing it; no third-party modules."""

import argparse
import hashlib
import json
import struct
from pathlib import Path


def inspect(path: Path) -> dict:
    data = path.read_bytes()

    def u16(offset):
        return struct.unpack_from("<H", data, offset)[0]

    def u32(offset):
        return struct.unpack_from("<I", data, offset)[0]

    if data[:2] != b"MZ":
        raise ValueError("Not a DOS/PE executable")
    pe = u32(60)
    if data[pe : pe + 4] != b"PE\0\0" or u16(pe + 4) != 0x8664:
        raise ValueError("Not a Windows x64 PE executable")
    optional = pe + 24
    if u16(optional) != 0x20B or u16(optional + 68) != 2:
        raise ValueError("Expected a PE32+ Windows GUI executable")
    section_table = optional + u16(pe + 20)
    sections = []
    for index in range(u16(pe + 6)):
        section = section_table + index * 40
        sections.append((u32(section + 12), max(u32(section + 8), u32(section + 16)), u32(section + 20)))

    def rva_to_offset(rva):
        for start, size, raw in sections:
            if start <= rva < start + size:
                offset = raw + rva - start
                if offset >= len(data):
                    raise ValueError("RVA exceeds file bounds")
                return offset
        raise ValueError("RVA is outside the PE sections")

    imports = []
    import_rva = u32(optional + 120)
    if import_rva:
        descriptor = rva_to_offset(import_rva)
        for _ in range(256):
            if not any(data[descriptor : descriptor + 20]):
                break
            name_offset = rva_to_offset(u32(descriptor + 12))
            name_end = data.index(b"\0", name_offset, min(name_offset + 512, len(data)))
            imports.append(data[name_offset:name_end].decode("ascii"))
            descriptor += 20
        else:
            raise ValueError("Import table exceeds inspection limit")
    runtime_imports = [name for name in imports if name.lower().startswith(("vcruntime", "msvcp", "ucrtbase", "api-ms-win-crt"))]
    if runtime_imports:
        raise ValueError(f"Dynamic MSVC runtime imports: {runtime_imports}")
    return {"file": path.name, "bytes": len(data), "architecture": "x64", "subsystem": "Windows GUI", "imports": sorted(imports), "dynamic_msvc_runtime": False, "sha256": hashlib.sha256(data).hexdigest()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = json.dumps(inspect(args.executable), indent=2) + "\n"
    if args.output:
        args.output.write_text(report, encoding="utf-8")
    print(report, end="")
