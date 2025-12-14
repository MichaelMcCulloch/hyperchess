
export const BASE_URL = "http://127.0.0.1:3123";

export interface ApiGameState {
    pieces: ApiPiece[];
    current_player: "White" | "Black";
    status: string;
    valid_moves: Record<string, ApiValidMove[]>;
    dimension: number;
    side: number;
    in_check: boolean;
    sequence: number;
}

export interface ApiPiece {
    piece_type: string;
    owner: "White" | "Black";
    coordinate: number[];
}

export interface ApiValidMove {
    to: number[];
    consequence: "Capture" | "NoEffect" | "Victory";
}

export interface NewGameRequest {
    mode: string;
    dimension: number;
    side: number;
}

export interface NewGameResponse {
    uuid: string;
}

export interface TurnRequest {
    uuid: string;
    start: number[];
    end: number[];
}

export const createGame = async (mode: string, dimension: number, side: number): Promise<NewGameResponse> => {
    const res = await fetch(`${BASE_URL}/new_game`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ mode, dimension, side }),
    });
    if (!res.ok) throw new Error("Failed to create game");
    return res.json();
};

export const getGame = async (uuid: string): Promise<ApiGameState> => {
    const res = await fetch(`${BASE_URL}/game/${uuid}`);
    if (!res.ok) throw new Error("Failed to get game");
    return res.json();
};

export const takeTurn = async (uuid: string, start: number[], end: number[]): Promise<ApiGameState> => {
    const res = await fetch(`${BASE_URL}/take_turn`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ uuid, start, end }),
    });
    if (!res.ok) throw new Error("Failed to move");
    return res.json();
};
