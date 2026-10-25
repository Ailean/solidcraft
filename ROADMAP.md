# SolidCraft roadmap

The short version, for people who use SolidCraft. The detailed developer status (milestone
tables, limitations, measurements) is in [docs/roadmap-detail.md](docs/roadmap-detail.md).

## What SolidCraft is

SolidCraft is open-source parametric 3D CAD written in Rust. You sketch, constrain, extrude, and
change a parameter to watch the timeline rebuild. It runs as a desktop app and in the browser
([try it](https://solidcraft.io/app)), and it has a command-line tool and an MCP server so that
scripts and AI agents can drive the same commands you use.

## Works today

- Sketching with a constraint solver, and the usual create and modify tools.
- Extrude, revolve, loft, sweep, coil, fillet, chamfer, shell, draft, holes, threads,
  patterns, mirror and split.
- Parameters with units and expressions, and configurations.
- Faces and edges keep their names through edits, so references survive changes.
- Components, assemblies, joints, motion studies and exploded views.
- Sheet metal with flat patterns and DXF export, and a plastic-enclosure subset.
- A Render workspace with GPU rendering, studio lighting, materials and image export
  (first release in 0.22.0).
- Import and export of STEP, IGES, 3MF, STL, OBJ and DXF, and FreeCAD and Rhino files.
- Autosave and crash recovery, and the browser build of the same app.

The full list of commands is in [docs/features.md](docs/features.md). Release notes for each
version are on the [releases page](https://github.com/tangency-labs/solidcraft/releases).

## In progress now

- **Rendering.** The Render workspace shipped in 0.22.0 and is still being filled out;
  the design notes are the `docs/render-*.md` files.
- **Drawings.** Drawings are not available yet. The groundwork is under way: the plan is in
  [#860](https://github.com/tangency-labs/solidcraft/issues/860) and the current slice is in
  [docs/drawing-foundations.md](docs/drawing-foundations.md). Today you can export the current
  view as a DXF (File, Export Current View as DXF).
- **Manufacturing proposals.** There is no CAM in SolidCraft. Proposals for it are being
  written and discussed in
  [#1153](https://github.com/tangency-labs/solidcraft/issues/1153) (milling, toolpaths and
  G-code posts) and
  [#1621](https://github.com/tangency-labs/solidcraft/issues/1621).
- **Assemblies.** Joints, motion studies and exploded views exist; work continues on how
  assemblies are built and edited. 0.22.0 grounded the first part and made free joint chains
  draggable.
- **SMLib kernel migration.** The geometry kernel is moving from truck to NVIDIA's
  SMLib (usd-brep, Apache-2.0). The work happens on the `kernel-smlib` branch and is not part
  of the releases yet; progress is tracked in
  [PR #4012](https://github.com/tangency-labs/solidcraft/pull/4012).

## Next

What comes next is planned a week at a time, in release milestones:

- **0.23** (due 2026-10-28): fixes and polish already in review.
- **0.24** (due 2026-11-04): the remaining crash reports, and the ribbon redesign, STEP import
  speed and Windows display work that are in progress.
- **0.25** (due 2026-11-11): to be chosen once 0.24 is out.

Anything without a milestone is not scheduled. The milestone due dates are the only dates
this page gives.

## How releases work

- Releases come often, usually every day or two, as numbered versions (v0.22.0, v0.22.1) with
  nightly pre-releases in between.
- SolidCraft tells you when a newer stable release exists and never updates without asking.
  Where it can replace itself safely (AppImage, Windows installer and portable builds) there is
  an **Update now** button; downloads are checked against the release's SHA-256 list. See
  [updates and privacy](docs/book/src/guide/updates.md).
- Design files carry a format number. A new version opens designs from older versions, but an
  older version refuses a design saved by a newer one (0.22.0 writes format 35, and 0.21.0 and
  earlier will not open it). Each release's notes say when the format changed. Keep a copy of
  an important design before you move to a new version.

## Licence

SolidCraft is licensed MIT OR Apache-2.0, at your option; see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE). Versions already released stay available under those licences
permanently.

## How to follow along

- Milestones: <https://github.com/tangency-labs/solidcraft/milestones>
- Issues: <https://github.com/tangency-labs/solidcraft/issues> (area labels such as
  `area:kernel`, `area:sketch` and `area:ui` group them)
- Discord: <https://discord.gg/FtU4NQNVDH>
- Releases: <https://github.com/tangency-labs/solidcraft/releases>
- Contributing: [CONTRIBUTING.md](CONTRIBUTING.md)
