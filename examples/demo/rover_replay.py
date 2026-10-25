#!/usr/bin/env python3
"""Replay a design's timeline in a running SolidCraft, for a screen recording (record_rover.sh).

    solidcraft --control 7878 &
    python3 examples/demo/rover_replay.py --port 7878 Rover-recorded.solidcraft [--seconds 110]

Opens the design, frames the finished model, rolls the timeline back to its start and then
forward in even steps (the History bar's roll-to, as `timeline.roll_to` over the control
channel) while the camera drifts slowly round the model; it ends on the finished design and a
slow turn. Prints "start" and "end" lines with epoch times so the recorder can trim.
"""
import argparse
import math
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from enclosure_mcp import control  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("design")
    ap.add_argument("--port", type=int, default=7878)
    ap.add_argument("--seconds", type=float, default=110, help="real time the timeline takes to play")
    ap.add_argument("--turn", type=float, default=8, help="seconds of slow turn on the finished design")
    a = ap.parse_args()

    def ui(method, params=None):
        r = control(a.port, method, params)
        if not r.get("ok", False):
            raise RuntimeError(f"{method} {params}: {r.get('error')}")
        return r.get("result")

    ui("ui.prefs", {"language": "en", "theme": "dark", "button": "apply"})
    ui("ui.resize", {"width": 1920, "height": 1080})
    ui("ui.home", {"open": False})
    ui("ui.file", {"path": os.path.abspath(a.design)})
    time.sleep(1)
    while ui("ui.inspect").get("file_busy"):
        time.sleep(0.5)
    ui("ui.home", {"open": False})
    n = ui("engine.execute", {"command": "timeline.roll_to", "params": {}})["features"]
    joints = control(a.port, "engine.execute", {"command": "joint.inspect", "params": {}}).get("result", {}).get("joints", [])
    ui("ui.set", {"showSketches": False, "showOrigin": False, "hiddenJoints": [j["id"] for j in joints if "id" in j]})
    ui("ui.view", {"view": "iso"})
    ui("ui.camera", {"fit": True})
    time.sleep(1.5)
    cam = ui("ui.inspect")["camera"]
    yaw0 = cam["yaw"]
    # Keep the finished model in frame all through the turn.
    ui("ui.camera", {"distance": cam["distance"] * 0.82})
    time.sleep(0.5)

    def execute(params):
        r = control(a.port, "engine.execute", {"command": "timeline.roll_to", "params": params})
        if not r.get("ok", False):
            raise RuntimeError(r)

    execute({"position": 0})
    time.sleep(1.0)
    print("start", time.time(), flush=True)
    total_turn = 0.9  # radians over the build
    t0 = time.time()
    steps = 90
    for i in range(1, steps + 1):
        t_target = t0 + a.seconds * i / steps
        pos = round(n * i / steps)
        execute({"position": pos} if pos < n else {})
        # The camera drifts a little every frame of the wait.
        while True:
            now = time.time()
            frac = min(1.0, (now - t0) / a.seconds)
            ui("ui.camera", {"yaw": yaw0 + total_turn * frac})
            if now >= t_target:
                break
            time.sleep(0.05)
    t1 = time.time()
    while time.time() - t1 < a.turn:
        f = (time.time() - t1) / a.turn
        ui("ui.camera", {"yaw": yaw0 + total_turn + 0.5 * f})
        time.sleep(0.04)
    print("end", time.time(), flush=True)


if __name__ == "__main__":
    main()
