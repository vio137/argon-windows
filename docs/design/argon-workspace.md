# Argon workspace rebuild

Original UI direction, requested 2026-11-09. Updated at 18:42 IST to glassmorphism throughout chrome and panels, per the user. Work on `ui-workspace-rebuild`, separate from the available preview.

## Direction

- Product: a browsing workspace around the Servo engine
- Character: precise, focused, tactile
- World: a quiet instrument panel around the live web
- Moments: the active page supplies the color; tab selection supplies a narrow mineral-green location marker
- Signature: a three-pixel vertical marker inside a clearly selected tab row

## Audit and passes

The previous shell mixed text-symbol icons, crowded address controls, and an undifferentiated sidebar. This rebuild replaces the toolbar, tab-row rendering, rail composition and settings surface. It keeps the engine and working state underneath.

1. Spacing: 4px rhythm, 32px icon controls, 36px tab rows, 248px rail
2. Color: near-neutral frosted graphite surfaces, three text roles, restrained mineral-green active selection
3. Icons: one original 18px path vocabulary, 1.5px strokes, accessible labels
4. Copy: actual commands and counts only, no promotional labels or invented content
5. Type: deliberate 11/12/13/14/18/24 hierarchy, no tiny negative tracking
6. Layout: product identity in the header, navigation adjoining a full-width address field, tab list grouped apart from window controls
7. Details: nested rounded controls, true selected and disabled states, full-title tooltips
8. Visuals: real webpages remain the rich content, no generated artwork or fake web previews
9. Motion: native egui feedback only; no decorative looping effects

## Compatibility and reuse

Servo remains the engine. Existing settings JSON, keyboard navigation, history, screenshots, media and tab commands are retained. Zen remains a reference for vertical tabs and compact navigation. Firefox-specific CSS from Zen theme-components was inspected but not transplanted into the Rust shell. No source from that component is copied or claimed as integrated.

## Attribution

Design process adapted from **unslop by Damir Sokolovsky**, https://github.com/heydamir/unslop, CC BY 4.0: https://creativecommons.org/licenses/by/4.0/. The process was adapted to an original native Rust/egui browser interface. No reference artwork is shipped. Implementation follows Ponytail's smallest-complete-change approach.

## Validation

Source and API inspection are not runtime proof. The branch needs compilation, real Windows pixel inspection, accessibility/keyboard interactions, settings persistence and media checks before any verified release claim.

## Glass composition

Windows acrylic is requested through window-vibrancy 0.8.1 (MIT or Apache-2.0), not simulated by alpha alone. Transparent native windows plus translucent chrome/panel fills expose the compositor backdrop while website rendering remains opaque. Actual acrylic availability, OpenGL alpha composition, settings overlays and legibility must be inspected on Windows. If the renderer does not expose the backdrop correctly, report that limitation and fix it before describing the result as working frosted glass. The user's explicit glass direction overrides unslop's default avoidance of glass decoration.
