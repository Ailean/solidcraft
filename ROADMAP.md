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

Each feature below is a [milestone](https://github.com/tangency-labs/solidcraft/milestones) holding
the issues and pull requests for it, so the count shows how far along it is. Counts are as of
2026-10-25 and are open issues and pull requests against closed ones; they are rough, not a promise.

- **[Rendering & materials](https://github.com/tangency-labs/solidcraft/milestone/17)**: in progress, 8 of 59 pieces done (51 open). The Render workspace shipped in 0.22.0; the interface rework and the material and light libraries are next.
- **[Drawings](https://github.com/tangency-labs/solidcraft/milestone/8)**: in progress, 2 of 40 pieces done (38 open). Not available yet. Sheets, borders and the paper scene are merged on the `drawings` branch; views, dimensions and exports are still being built. Today you can export the current view as a DXF.
- **[Sheet metal](https://github.com/tangency-labs/solidcraft/milestone/15)**: in progress, 1 of 34 pieces done (33 open). Flanges, reliefs and jogs are being extended.
- **[Sketching](https://github.com/tangency-labs/solidcraft/milestone/14)**: in progress, 12 of 56 pieces done (44 open). Dimension types, selection and snapping options.
- **[Solid modeling](https://github.com/tangency-labs/solidcraft/milestone/2)**: in progress, 9 of 58 pieces done (49 open). Blends, holes, threads and patterns.
- **[Assemblies & joints](https://github.com/tangency-labs/solidcraft/milestone/3)**: in progress, 5 of 23 pieces done (18 open). Joints and motion work today; placement rules, fasteners and weldments are being added.
- **[Import & export](https://github.com/tangency-labs/solidcraft/milestone/5)**: in progress, 18 of 60 pieces done (42 open). FreeCAD, SolidWorks and point-cloud import, and mesh export options.
- **[AI agents & MCP](https://github.com/tangency-labs/solidcraft/milestone/18)**: in progress, 2 of 19 pieces done (17 open). Agent chat and the MCP server.
- **[Interface, web app & plugins](https://github.com/tangency-labs/solidcraft/milestone/4)**: in progress, 35 of 121 pieces done (86 open). Ribbon redesign, navigation, themes, the web app and plugins.
- **[SMLib kernel migration](https://github.com/tangency-labs/solidcraft/milestone/1)**: in progress, 3 of 5 pieces done (2 open). The kernel is moving from truck to NVIDIA's SMLib (usd-brep, Apache-2.0) on the `kernel-smlib` branch; it is not in releases yet. Tracked in [#4012](https://github.com/tangency-labs/solidcraft/issues/4012).

## Next

- **[Surfacing](https://github.com/tangency-labs/solidcraft/milestone/16)**: next, 6 of 15 pieces done (9 open). Patch, trim, blend and match exist; the rest of the surface toolset is planned.
- **[Manufacturing / CAM](https://github.com/tangency-labs/solidcraft/milestone/9)**: next, 0 of 3 pieces done (3 open). There is no CAM in SolidCraft yet. Send to Slicer and a first toolpath slice are in review; proposals are [#1153](https://github.com/tangency-labs/solidcraft/issues/1153) and [#1621](https://github.com/tangency-labs/solidcraft/issues/1621).

Features are not tied to a release or a date. Work lands in the daily releases as it is finished.
Anything without a milestone, such as bug fixes, is not a feature and is not listed here.

## How releases work

- Releases ship daily, as numbered versions (v0.22.0, v0.22.1) with nightly pre-releases in
  between. They are delivered through the in-app updater.
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

- Feature milestones: <https://github.com/tangency-labs/solidcraft/milestones>
- Issues: <https://github.com/tangency-labs/solidcraft/issues> (area labels such as
  `area:kernel`, `area:sketch` and `area:ui` group them)
- Discord: <https://discord.gg/FtU4NQNVDH>
- Releases: <https://github.com/tangency-labs/solidcraft/releases>
- Contributing: [CONTRIBUTING.md](CONTRIBUTING.md)
