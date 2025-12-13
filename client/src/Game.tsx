import { useCallback, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { type ApiGameState, getGame, takeTurn } from "./api";
import "./Game.css";

// Import SVGs
import wP from "./assets/pieces/wP.svg";
import bP from "./assets/pieces/bP.svg";
import wR from "./assets/pieces/wR.svg";
import bR from "./assets/pieces/bR.svg";
import wN from "./assets/pieces/wN.svg";
import bN from "./assets/pieces/bN.svg";
import wB from "./assets/pieces/wB.svg";
import bB from "./assets/pieces/bB.svg";
import wQ from "./assets/pieces/wQ.svg";
import bQ from "./assets/pieces/bQ.svg";
import wK from "./assets/pieces/wK.svg";
import bK from "./assets/pieces/bK.svg";

const PIECE_SVGS: Record<string, { w: string, b: string }> = {
    "Pawn": { w: wP, b: bP },
    "Rook": { w: wR, b: bR },
    "Knight": { w: wN, b: bN },
    "Bishop": { w: wB, b: bB },
    "Queen": { w: wQ, b: bQ },
    "King": { w: wK, b: bK }
};

interface SliceProps {
    state: ApiGameState;
    selected: number[] | null;
    onSquareClick: (coords: number[]) => void;
    currentDim: number;
    fixedCoords: Record<number, number>;
}

const RecursiveGrid = ({ state, selected, onSquareClick, currentDim, fixedCoords }: SliceProps) => {
    if (currentDim < 2) {
        return <Board2D state={state} selected={selected} onSquareClick={onSquareClick} fixedCoords={fixedCoords} />;
    }

    const targetIndex = currentDim; 
    const slices = [];
    const side = state.side;

    for (let i = 0; i < side; i++) {
        const nextFixed = { ...fixedCoords, [targetIndex]: i };
        
        slices.push(
            <div key={`slice-${targetIndex}-${i}`} className="dimension-slice">
                <div className="slice-label">
                    {(() => {
                        // Dim 2 (Index 2) -> Z. Dim 3 (Index 3) -> W. Dim 4+ -> D{Index+1}
                        if (targetIndex === 2) return `Z=${i + 1}`;
                        if (targetIndex === 3) return `W=${i + 1}`;
                        return `D${targetIndex + 1}=${i + 1}`;
                    })()}
                </div>
                <RecursiveGrid 
                    state={state} 
                    selected={selected} 
                    onSquareClick={onSquareClick} 
                    currentDim={currentDim - 1} 
                    fixedCoords={nextFixed} 
                />
            </div>
        );
    }

    // CLI Layout: 3D = Row. 4D = Col of Rows.
    const isRow = targetIndex % 2 === 0; 
    return (
        <div className={`recursive-grid ${isRow ? "row-layout" : "col-layout"}`}>
            {slices}
        </div>
    );
};

interface Board2DProps {
    state: ApiGameState;
    selected: number[] | null;
    onSquareClick: (coords: number[]) => void;
    fixedCoords: Record<number, number>;
}

const Board2D = ({ state, selected, onSquareClick, fixedCoords }: Board2DProps) => {
    const side = state.side;
    const pieces = state.pieces;
    const validMoves = selected ? state.valid_moves[`(${selected.join(", ")})`] : [];

    const squares = [];
    
    // 90 degrees Clockwise Rotation (corrected for White on Left):
    // Screen Row (0..side-1) -> File (0..side-1) [Top is A]
    // Screen Col (0..side-1) -> Rank (0..side-1) [Left is 1]
    for (let row = 0; row < side; row++) {
        for (let col = 0; col < side; col++) {
            const c = row; // File
            const r = col; // Rank

            const isMatch = (pCoords: number[]) => {
                if (pCoords[0] !== r || pCoords[1] !== c) return false;
                for (const [dim, val] of Object.entries(fixedCoords)) {
                    if (pCoords[parseInt(dim)] !== val) return false;
                }
                return true;
            };

            const piece = pieces.find(p => isMatch(p.coordinate));
            const isSelected = selected && isMatch(selected);
            const targetMove = validMoves?.find(m => {
                if (m.to[0] !== r || m.to[1] !== c) return false;
                for (const [dim, val] of Object.entries(fixedCoords)) {
                    if (m.to[parseInt(dim)] !== val) return false;
                }
                return true;
            });

            // Checkering pattern: (r+c)%2 works for standard board.
            const isDark = (r + c) % 2 === 0; 
            
            const clickHandler = () => {
                const coord = [];
                for (let i = 0; i < state.dimension; i++) {
                    if (i === 0) coord.push(r);
                    else if (i === 1) coord.push(c);
                    else coord.push(fixedCoords[i] || 0);
                }
                onSquareClick(coord);
            };

            squares.push(
                <div 
                    key={`${r}-${c}`}
                    className={`square ${isDark ? "dark" : "light"} ${isSelected ? "selected" : ""} ${targetMove ? "target" : ""} ${targetMove?.consequence === "Capture" ? "capture" : ""}`}
                    onClick={clickHandler}
                >
                    {piece && <PieceDisplay type={piece.piece_type} owner={piece.owner} />}
                    {targetMove && !piece && <div className="dot"></div>}
                </div>
            );
        }
    }

    return (
        <div 
            className="chess-board"
            style={{ 
                gridTemplateColumns: `repeat(${side}, 1fr)`,
                gridTemplateRows: `repeat(${side}, 1fr)` 
            }}
        >
            {squares}
        </div>
    );
};

const PieceDisplay = ({ type, owner }: { type: string, owner: string }) => {
    const colorKey = owner === "White" ? "w" : "b";
    const src = PIECE_SVGS[type]?.[colorKey];
    if (!src) return <span>?</span>;
    return (
        <div className={`piece ${owner.toLowerCase()}`}>
            <img src={src} alt={`${owner} ${type}`} />
        </div>
    );
};

const Game = () => {
    const { uuid } = useParams<{ uuid: string }>();
    const [gameState, setGameState] = useState<ApiGameState | null>(null);
    const [selectedSquare, setSelectedSquare] = useState<number[] | null>(null);
    const [error, setError] = useState("");

    const fetchState = useCallback(async () => {
        if (!uuid) return;
        try {
            const state = await getGame(uuid);
            setGameState(state);
        } catch (e) {
            console.error(e);
            setError("Failed to load game");
        }
    }, [uuid]);

    useEffect(() => {
        fetchState();
        const interval = setInterval(fetchState, 1000); 
        return () => clearInterval(interval);
    }, [fetchState]);

    const handleSquareClick = async (coord: number[]) => {
        if (!gameState) return;
        
        if (selectedSquare) {
            const fromKey = `(${selectedSquare.join(", ")})`;
            const validMoves = gameState.valid_moves[fromKey];
            const isTarget = validMoves?.some(m => JSON.stringify(m.to) === JSON.stringify(coord));
            
            if (isTarget) {
                try {
                    const newState = await takeTurn(uuid!, selectedSquare, coord);
                    setGameState(newState);
                    setSelectedSquare(null);
                } catch (e) {
                    console.error(e);
                    alert("Move failed");
                }
                return;
            }
        }
        
        const piece = gameState.pieces.find(p => JSON.stringify(p.coordinate) === JSON.stringify(coord));
        if (piece && piece.owner === gameState.current_player) {
            setSelectedSquare(coord);
        } else {
            setSelectedSquare(null);
        }
    };

    if (error) return <div className="error">{error}</div>;
    if (!gameState) return <div className="loading">Loading...</div>;

    const maxIndex = gameState.dimension - 1;

    return (
        <div className="game-wrapper">
             <div className="turn-info">
                Turn: <span className={gameState.current_player.toLowerCase()}>{gameState.current_player}</span>
                {gameState.in_check && <span className="check-badge">CHECK</span>}
                {gameState.status !== "InProgress" && <div className="game-over">{gameState.status}</div>}
             </div>
             
             {gameState.dimension === 2 ? (
                 <Board2D 
                    state={gameState} 
                    selected={selectedSquare} 
                    onSquareClick={handleSquareClick}
                    fixedCoords={{}}
                 />
             ) : (
                <RecursiveGrid 
                    state={gameState}
                    selected={selectedSquare}
                    onSquareClick={handleSquareClick}
                    currentDim={maxIndex}
                    fixedCoords={{}}
                />
             )}
        </div>
    );
};

export default Game;
