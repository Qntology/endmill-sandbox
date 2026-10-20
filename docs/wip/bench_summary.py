"""Compact view of literature_benchmark.csv (and optionally diff against a baseline run).

usage: python3 bench_summary.py <dir> [<baseline_dir>]
"""
import csv
import math
import sys


def load(d):
    rows = {}
    with open(f"{d}/literature_benchmark.csv", newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            rows[r["id"]] = r
    return rows


def main():
    cur = load(sys.argv[1])
    base = load(sys.argv[2]) if len(sys.argv) > 2 else {}
    inb = 0
    life_err = []
    for k, r in cur.items():
        pred = float(r["predicted"])
        meas = float(r["measured"])
        ok = r["in_band"] == "true"
        inb += ok
        b = base.get(k)
        bp = f"{float(b['predicted']):9.3g}{'o' if b['in_band'] == 'true' else 'x'}" if b else ""
        if r["kind"] == "절댓값" and r["quantity"] in ("life_min", "life_volume_cm3"):
            life_err.append(abs(math.log10(pred / meas)) if pred > 0 else 9)
        print(f"{'o' if ok else 'x'} {r['kind'][:3]:3s} {k:22s} meas {meas:9.3g} [{float(r['lo']):8.3g},{float(r['hi']):8.3g}] pred {pred:9.3g} ratio {float(r['ratio']):7.3f} {bp}")
    life_err.sort()
    med = life_err[len(life_err) // 2] if life_err else float("nan")
    print(f"in band {inb}/{len(cur)}  abs-life median |log10| {med:.3f} (x{10**med:.2f}) n={len(life_err)}")
    if base:
        binb = sum(1 for r in base.values() if r["in_band"] == "true")
        print(f"baseline in band {binb}/{len(base)}")


if __name__ == "__main__":
    main()
