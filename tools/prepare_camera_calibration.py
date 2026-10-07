"""Prepare private per-image calibration using a locally installed DNG Converter.

Only reduced-resolution Sony linear ARWs are supported in this first version.
Reads originals; conversion goes into a temporary directory. No images or Adobe
profiles are bundled or uploaded. Restart CJV after preparing new calibrations.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import tempfile


def read_tags(path):
    data = Path(path).read_bytes()
    if data[:4] not in (b"II*\0", b"MM\0*"):
        raise ValueError("Expected classic TIFF/DNG")
    order = "<" if data[:2] == b"II" else ">"
    tags, visited = {}, set()
    wanted = {271,272,262,277,50721,50722,50723,50724,50727,50728,50730,
              50778,50779,50931,50932,50964,50965,51109,330}

    def read_ifd(offset):
        if not offset or offset in visited:
            return
        if len(visited) >= 16 or offset + 2 > len(data):
            raise ValueError("Invalid TIFF directory")
        visited.add(offset)
        count = struct.unpack_from(order + "H", data, offset)[0]
        if count > 4096 or offset + 2 + count * 12 + 4 > len(data):
            raise ValueError("Invalid TIFF entry count")
        children = []
        for i in range(count):
            entry = offset + 2 + i * 12
            tag, kind, n, position = struct.unpack_from(order + "HHII", data, entry)
            if tag not in wanted:
                continue
            size = {1:1,2:1,3:2,4:4,5:8,9:4,10:8,11:4,12:8}.get(kind)
            if size is None or n > 4096:
                raise ValueError("Unsupported or excessive calibration value")
            position = entry + 8 if n * size <= 4 else position
            if position + n * size > len(data):
                raise ValueError("Truncated calibration value")
            raw = data[position:position + n * size]
            if kind == 2:
                value = raw.rstrip(b"\0").decode("utf-8",errors="strict")
            elif kind in (5,10):
                pairs = struct.iter_unpack(order + ("II" if kind == 5 else "ii"),raw)
                value = [a / b for a,b in pairs]
            else:
                value = list(struct.unpack(order + {1:"B",3:"H",4:"I",9:"i",11:"f",12:"d"}[kind] * n,raw))
            if tag == 330:
                children.extend(value)
            else:
                tags[tag] = value
        for child in children:
            read_ifd(child)
    read_ifd(struct.unpack_from(order + "I",data,4)[0])
    return tags


def calibration(source, dng, forward=False):
    raw_tags, t = read_tags(source), read_tags(dng)
    if source.suffix.lower() != ".arw" or "sony" not in str(raw_tags.get(271,"")).lower():
        raise ValueError("Only Sony ARW sources are supported")
    if t.get(262) != [34892] or t.get(277) != [3] or t.get(50728) != [1.0,1.0,1.0]:
        raise ValueError("Expected a reduced-resolution Sony linear RAW with baked white balance")
    if raw_tags.get(272) != t.get(272):
        raise ValueError("Source and DNG camera models differ")
    if t.get(50778) != [17] or t.get(50779) != [21]:
        raise ValueError("Only dual A/D65 calibration is currently supported")
    if t.get(50931,"") != t.get(50932,""):
        raise ValueError("Camera and profile calibration signatures differ")
    result = {"schema":1, "enabled":True, "sourceSha256":hashlib.sha256(source.read_bytes()).hexdigest(),
              "inputKind":"sony-linear-arw", "model":t[272], "useForwardMatrix":forward,
              "provenance":"Metadata from a local Adobe DNG Converter conversion of this exact source"}
    for tag,key,n in [(50721,"colorMatrix1",9),(50722,"colorMatrix2",9),
        (50723,"cameraCalibration1",9),(50724,"cameraCalibration2",9),
        (50964,"forwardMatrix1",9),(50965,"forwardMatrix2",9),
        (50727,"analogBalance",3),(50728,"asShotNeutral",3)]:
        value = t[tag]
        if len(value) != n or not all(math.isfinite(v) for v in value):
            raise ValueError("Invalid " + key)
        result[key] = value
    result["baselineExposure"] = t.get(50730,[0])[0] + t.get(51109,[0])[0]
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sources",nargs="+",type=Path)
    p.add_argument("--converter",type=Path,default=Path(os.environ.get("ProgramFiles","C:/Program Files"))/"Adobe/Adobe DNG Converter/Adobe DNG Converter.exe")
    p.add_argument("--cache",type=Path,default=Path(os.environ.get("APPDATA","."))/"au.com.cameronjonesvisuals.cjvstudio.alpha/camera-calibration")
    p.add_argument("--forward",action="store_true",help="Experimental ForwardMatrix rendering; validate before enabling")
    args = p.parse_args()
    if not args.converter.is_file():
        p.error("Install Adobe DNG Converter or provide --converter")
    args.cache.mkdir(parents=True,exist_ok=True)
    for source in args.sources:
        source = source.resolve(strict=True)
        before = hashlib.sha256(source.read_bytes()).hexdigest()
        with tempfile.TemporaryDirectory(prefix="cjv-calibration-") as temp:
            result = subprocess.run([str(args.converter),"-c","-p0","-d",temp,str(source)],
                capture_output=True,timeout=180,creationflags=getattr(subprocess,"CREATE_NO_WINDOW",0))
            converted = list(Path(temp).glob("*.dng"))
            if result.returncode != 0 or len(converted) != 1:
                raise RuntimeError("DNG conversion failed for " + source.name)
            profile = calibration(source,converted[0],args.forward)
            if profile["sourceSha256"] != before:
                raise RuntimeError("Source changed during preparation")
            destination = args.cache / (before + ".json")
            # Existing calibration is kept; produce a reviewable new file.
            with destination.open("x",encoding="utf-8") as output:
                json.dump(profile,output,indent=2,allow_nan=False)
            print("Prepared " + source.name + ": " + str(destination))


if __name__ == "__main__":
    main()
