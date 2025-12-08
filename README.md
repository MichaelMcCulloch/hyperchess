# HyperTicTacToe

Experimenting with higher-dimensional Tic-Tac-Toe using Rust, optimized bitboards, and Domain-Driven Design.

## Overview

HyperTicTacToe is a high-performance implementation of Tic-Tac-Toe that scales to N-dimensions (2D, 3D, 4D, etc.).

### Classic
```
O X O
O X X
X O X
It's a Draw!
```
### 3D
```
O . .| X X .| . . .
. . .| . X .| . . .
. . O| . X O| . . .
Player X Wins!
```

### 4D
```
O . .| . . .| . . .
. . .| O . O| . . .
. . .| . . .| . . .
-------------------
. . .| . . .| . . .
. . .| X X X| . . .
. . .| . . .| . . .
-------------------
. . .| . . .| . . .
. . .| X . .| . . .
. . .| . . .| . . .
Player X Wins!
```

### 5D
```
X . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .
. . .| . . .| . . .| . . .| . O .| . . .| . . .| . . .| . . .
. . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .
-------------------| -------------------| -------------------
. . .| . . .| . . .| X . .| . . .| . . .| . . .| . . .| . . .
. . .| . O .| . . .| . . .| . X .| . . .| . . .| . . .| . . .
. . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .
-------------------| -------------------| -------------------
. . .| . . .| . . .| . . .| . . .| . . .| X . .| . . .| . . .
. . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .
. . .| . . .| . . .| . . .| . . .| . . .| . . .| . . .| . . O
Player X Wins!
```
*Optimal play from both players essentially guarantees X a win in higher dimensions.*
## Usage

### Prerequisites
- Rust

### Running the Game

To play a 3D game vs the computer:

```bash
cargo run
```

### Command Line Arguments

```bash
RUSTFLAGS="-C target-cpu=native" cargo run --release -- [dimension] [mode] [depth]
```

- **dimension**: The dimension of the board (e.g., `2` for normal Tic-Tac-Toe, `3` for 3D). Default is `3`.
- **mode**: A two-character string specifying players for X and O.
    - `h`: Human
    - `c`: Computer (Minimax)
    - Example: `hc` (Human vs Computer), `cc` (Computer vs Computer).
- **depth**: Minimax search depth limit.

**Examples:**

Play 2D Tic-Tac-Toe against the bot:
```bash
cargo run -- 2 hc
```

Watch two bots play in 3D:
```bash
RUSTFLAGS="-C target-cpu=native" cargo run --release -- 3 cc
```

```
