# HyperChess

HyperChess is a generalized N-dimensional chess engine written in Rust. It extends the classic game of Chess to arbitrary dimensions (2D, 3D, 4D, etc.) using a consistent set of geometric rules.

## Rules of N-Dimensional Chess

### 1. The Board & Coordinates
* **Dimensions:** The board exists in $N$ dimensions (e.g., 2D, 3D, 5D).
* **Size:** Each dimension has a side length $S$ (Standard Chess: $8 \times 8$; HyperChess default: $8 \times 8 \dots$ ($N$ times)).
* **Coordinates:** A square is identified by a vector of coordinates $C = [c_0, c_1, \dots, c_{n-1}]$.
    * **Axis 0 (Rank):** Corresponds to the "Forward/Backward" direction for White/Black.
    * **Axis 1 (File):** Corresponds to the "Lateral/Sideways" direction (Standard Left/Right).
    * **Axes 2+:** Correspond to higher dimensions (e.g., "Height", "Hyper-Height").

### 2. Movement & Capture Rules
All pieces capture by landing on a square occupied by an enemy piece, replacing it.

#### Rook (Orthogonal Slider)
* **Movement:** Moves any distance along any **single axis**.
* **Rule:** Valid if exactly **one** coordinate changes value. All other coordinates must remain constant.
* **Capture:** Standard displacement capture.
* **Visual:** In 3D, a Rook moves along columns (up/down), ranks (forward/back), or files (left/right).

#### Bishop (Colorbound Diagonal Slider)
* **Movement:** Moves any distance along generalized diagonals.
* **Rule:** Valid if the number of coordinates that change is **non-zero and even**. The magnitude of change must be equal for all changing coordinates ($\Delta c_i = \Delta c_j$ for all changing axes).
* **Constraint:** This rule preserves "colorbinding" (staying on squares of the same color) in any dimension.
    * *Note: In 3D, a move changing all 3 coordinates (Space Diagonal) is invalid because 3 is odd.*

#### Queen (Combined Slider)
* **Movement:** Combines the movement of the Rook and Bishop.
* **Rule:** Valid if it follows the rules of either a **Rook** (1 axis changes) or a **Bishop** (Even number of axes change).

#### Knight (Leaper)
* **Movement:** Moves in an "L" shape in any 2D plane defined by two axes.
* **Rule:** Changes exactly **one** coordinate by $\pm 2$ and exactly **one other** coordinate by $\pm 1$. All other coordinates remain unchanged.
* **Leaping:** Jumps over intervening pieces.

#### King (Adjacency Leaper)
* **Movement:** Moves to any adjacent square.
* **Rule:** Changes any number of coordinates by $\pm 1$ or $0$. (Chebyshev distance = 1).
* **Restriction:** Cannot move into check.

### 3. Pawn Mechanics ("Super Pawn")

#### Movement (Pushes)
* **Direction:**
    * **White:** Moves $+1$ (Positive direction).
    * **Black:** Moves $-1$ (Negative direction).
* **Allowed Axes:**
    * A Pawn can treat **any axis** as "forward" **EXCEPT Axis 1 (File/Lateral)**.
    * *Valid:* Pushing along Rank (Axis 0) or Height (Axis 2+).
    * *Invalid:* Pushing sideways along File (Axis 1).
* **Single Push:** Moves 1 step forward on a valid axis to an empty square.
* **Double Push:** Moves 2 steps forward on a valid axis if:
    * The path is clear.
    * The pawn is on its **starting rank** for that specific axis (Coordinate $= 1$ for White, $S-2$ for Black).

#### Captures
* **Rule:** A pawn captures by moving **diagonally** in a specific way:
    1.  Moves $+1$ (forward) along a valid **movement axis** (e.g., Rank or Height).
    2.  Moves $\pm 1$ along exactly **one other axis** (the "Capture Axis").
* **Example (3D):** A White Pawn at $(1, 1, 1)$ can capture an enemy at:
    * $(2, 2, 1)$ [Move Rank +1, Side +1]
    * $(2, 1, 2)$ [Move Rank +1, Height +1]
    * $(1, 1, 2)$ is a *push* (Height +1), not a capture.

#### Promotion
* **Trigger:** Reaching the far end of the lattice.
    * **White:** Must reach coordinate $S-1$ on **ALL axes except Axis 1 (File/Lateral)**.
    * **Black:** Must reach coordinate $0$ on **ALL axes except Axis 1 (File/Lateral)**.
* **Note:** Moving to the end of just one dimension (e.g., just Rank) is **not** sufficient for promotion in 3D+.
* **Result:** Promotes to Queen, Rook, Bishop, or Knight.

### 4. Special Moves

#### En Passant
* **Condition:**
    1.  Enemy pawn executes a **Double Push** on Axis $X$.
    2.  Your pawn is positioned such that it could capture the skipped square via a standard capture move.
* **Execution:** Move to the "skipped" square behind the enemy pawn. The enemy pawn is removed.
* **Timing:** Must be done immediately on the turn following the double push.

#### Castling
* **Axes:** Strictly occurs along **Axis 1 (File)**.
* **Logic:**
    * **Kingside:** King moves from File 4 to File 6. Rook moves from File 7 to File 5.
    * **Queenside:** King moves from File 4 to File 2. Rook moves from File 0 to File 3.
* **Requirements:**
    1.  King and chosen Rook have never moved.
    2.  Path between them is empty.
    3.  King is not in check, does not pass through check, and does not land in check.
    4.  Coordinates on all other axes (Rank, Height, etc.) must match (King and Rook must be "aligned").

## Usage

### Prerequisites
- Rust (latest stable)

### Running the Game
Run the game via `cargo`:

```bash
cargo run --release -- [dimension] [player_mode] [depth]
```

**Arguments:**

1.  **dimension** (Optional): The number of spatial dimensions for the board.
    *   **Default:** `2` (Standard Chess)
    *   **Values:** `2`, `3`, `4`, etc.
2.  **player_mode** (Optional): Specifies the types of the two players (White and Black).
    *   **Default:** `hc` (Human vs Computer)
    *   **Format:** A two-character string (e.g., `cc`, `hh`).
        *   First character: White player (`h` = Human, `c` = Computer).
        *   Second character: Black player.
3.  **depth** (Optional): The search depth for the Computer AI.
    *   **Default:** `4`
    *   **Note:** Higher depth significantly increases calculation time.

**Examples:**

```bash
# Play Standard 2D Chess (Human vs Computer) - Uses defaults
cargo run --release

# Play 3D Chess (Human vs Computer)
cargo run --release -- 3

# Watch a 3D Chess match between two bots
cargo run --release -- 3 cc

# Play 2D Chess against a stronger bot (Depth 6)
cargo run --release -- 2 hc 6
```

### Move Input Format (Console)
When playing as a human, enter moves using **Linear Indices**.

Format: `FromIndex ToIndex [Promotion]`

*   **FromIndex**: The linear index of the piece you want to move.
*   **ToIndex**: The linear index of the destination square.
*   **Promotion** (Optional): If promoting a pawn, specify the piece type: `Q` (Queen), `R` (Rook), `B` (Bishop), `N` (Knight).

#### Coordinate System & Indices
The board is flattened into a linear array.
*   **Index Formula:** $Index = \sum_{i=0}^{N-1} (c_i \cdot S^i)$
    *   $c_i$: Coordinate on Axis $i$.
    *   $S$: Side length of the board.

**Axis Roles:**
*   **Axis 0 (Rank):** Increments by $1$.
*   **Axis 1 (File):** Increments by $S$.
*   **Axis 2 (Height):** Increments by $S^2$.
*   **Axis $k$:** Increments by $S^k$.

---

#### Examples

**1. 2D Board ($8 \times 8$)**
*   **Side $S = 8$**
*   Formula: $Index = c_0 + 8c_1$
*   **Pos (0, 0):** $0$
*   **Pos (3, 4):** $3 + (4 \times 8) = 35$
*   **Pos (7, 7):** $7 + (7 \times 8) = 63$

**2. 3D Board ($8 \times 8 \times 8$)**
*   **Side $S = 8$**
*   Formula: $Index = c_0 + 8c_1 + 64c_2$
*   **Pos (0, 0, 0):** $0$
*   **Pos (4, 4, 4):** $4 + 32 + (4 \times 64) = 292$
*   **Pos (7, 7, 7):** $7 + 56 + 448 = 511$

**3. 5D Board ($4 \times 4 \times 4 \times 4 \times 4$)**
*   **Side $S = 4$**
*   Formula: $Index = c_0 + 4c_1 + 16c_2 + 64c_3 + 256c_4$
*   **Pos (1, 2, 0, 3, 1):**
    *   $c_0=1$
    *   $c_1=2 \to 8$
    *   $c_2=0$
    *   $c_3=3 \to 192$
    *   $c_4=1 \to 256$
    *   **Total:** $1 + 8 + 0 + 192 + 256 = 457$

---

**Move Example:**
*   `8 16` : Move piece from index 8 to index 16.
*   `50 58 Q` : Move pawn from 50 to 58 and promote to Queen.
