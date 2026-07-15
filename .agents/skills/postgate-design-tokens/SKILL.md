---
name: postgate-design-tokens
description: Apply PostGate's desktop application design tokens to product UI, documentation, screenshots, and visual assets. Use when creating or reviewing PostGate interfaces, changing colors or typography, styling request and response states, adding Liquid Glass surfaces, or selecting the official application icon.
---

# PostGate Design Tokens

Keep every PostGate surface visually connected to the desktop application. Treat the app theme files as the source of truth and use the packaged Gate icon rather than legacy radar artwork.

## Workflow

1. Read `references/tokens.md` before changing visual styles.
2. Inspect the current source files listed there when modifying the desktop theme itself. Update the reference if the source tokens change.
3. Map structural surfaces to the neutral zinc palette. Do not introduce a separate brand hue.
4. Reserve emerald, blue, amber, and red for their documented method and status meanings.
5. Use the editor palette only for Whistle rule syntax or code examples that reproduce that editor.
6. Use `assets/postgate-icon.png` for product identity. Preserve its aspect ratio and rounded-square silhouette; do not recolor or redraw it.
7. Verify light and dark themes at desktop and mobile widths. Check focus, hover, disabled, loading, empty, success, warning, and error states when applicable.

## Product Character

- Keep operational surfaces compact, flat, and easy to scan.
- Let typography, spacing, borders, and hierarchy carry the design.
- Use translucent material only for navigation, floating toolbars, menus, and modal surfaces. Keep content sections unframed.
- Use an 8px maximum corner radius unless an existing component has a stricter local convention.
- Use Lucide icons for interface actions. Prefer symbols over text when the action is familiar and add a tooltip when its meaning is not obvious.
- Avoid decorative gradients, colored glows, oversized cards, and broad washes of semantic colors.

## Resources

- Read `references/tokens.md` for exact color, typography, radius, editor, and usage mappings.
- Copy `assets/postgate-icon.png` when an official PostGate icon is needed.
