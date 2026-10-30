# Drawn positions are derived from placed positions

From Phase 8 a shape's drawn position is a pure result of the spacing pass over placed positions, sizes, links and the theme's gaps; pushes are never saved or undone, and this replaces the saved pushes of ADR 0004. We chose a pure pass over a session that remembers each shape's offset because a document then has one picture per theme: reopening, undo and redo need no settling logic, and a theme switch leaves the document saved. The cost: the pass can't know which shape just changed, so it can't honor "the shape that grew pushes the others". Instead the larger chain holds still, then the chain whose top shape is highest.

## Consequences

- Direction words and story order read drawn positions, and only `@layout` reads placed ones. Placed positions alone give wrong answers: a shape that grows past its neighbor's placed position would turn `below` into `above`, reversing story order (ADR 0002). So the word order of saved text can vary by theme.
- A few changes still write placed positions, because the pass can't work them out. Insert-and-push saves its subtree move: only the command knows which shape goes below which, and without the move the two would share a position. A move first takes the touched chains' drawn positions as placed, so a drop lands where it was released, and those chains stop settling back.
- Spacing never changes zone membership. Membership is saved and explicit, and pushes aren't.
- Opening a document measures every shape before anything is drawn, because the pass needs every height.
