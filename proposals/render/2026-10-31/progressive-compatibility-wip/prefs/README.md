# Preference UI and locale successor WIP

Two RAW candidates: prefs.rs from the sealed preferences proposal and the exact
locale-only v2 successor. **NOT ADOPTED, UNCOMPILED, UNRUN**. One new preference
contract test is written, not executed. Both bools default true, persist through
the existing typed protocol, reject nonboolean values and reset with Graphics.
Unrelated page reset preserves them. The collapsed Advanced CPU rendering
section uses existing UI-kit controls. Runtime consumers require core + ui.

The locale successor changes one help message in six languages, describing the
earlier sparse policy without promising powers-of-two for protected rendering.
Publication can be coalesced by the display; no physical-repaint guarantee is
made. Other message values remain exact. The new locale has no original base
file; the intermediate v1 hash is recorded only as lineage in the bindings.
