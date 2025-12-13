import urllib.request
import urllib.error
import json
import time
import sys

BASE_URL = "http://127.0.0.1:3123"

def post(endpoint, data):
    url = f"{BASE_URL}{endpoint}"
    req = urllib.request.Request(url, method="POST")
    req.add_header('Content-Type', 'application/json')
    json_data = json.dumps(data).encode('utf-8')
    try:
        with urllib.request.urlopen(req, data=json_data) as response:
            return json.loads(response.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        print(f"HTTP Error {e.code}: {e.reason}")
        print(e.read().decode('utf-8'))
        sys.exit(1)
    except urllib.error.URLError as e:
        print(f"URL Error: {e.reason}")
        sys.exit(1)

def get(endpoint):
    url = f"{BASE_URL}{endpoint}"
    req = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        print(f"HTTP Error {e.code}: {e.reason}")
        sys.exit(1)

def print_board(state):
    dim = state['dimension']
    side = state['side']
    pieces = state['pieces']
    
    # Simple 2D print if dim=2
    if dim == 2:
        board = [['.' for _ in range(side)] for _ in range(side)]
        for p in pieces:
            coord = p['coordinate']
            r, c = coord[0], coord[1]
            # Map standard view: rank 0 is bottom, file 0 is left.
            # We print top to bottom (rank 7 down to 0).
            ptype = p['piece_type']
            owner = p['owner']
            char = ptype[0].upper() if ptype != "Knight" else "N"
            if owner == "Black":
                char = char.lower()
            if 0 <= r < side and 0 <= c < side:
                board[r][c] = char
        
        print("  " + " ".join([str(i) for i in range(side)]))
        for r in range(side - 1, -1, -1):
            print(f"{r} " + " ".join(board[r]))
    else:
        print(f"Multidimensional board ({dim}D), showing raw pieces count: {len(pieces)}")

def main():
    print("--- HyperChess API Driver ---")
    
    # 1. Create New Game
    print("\n1. Creating New Game (Human vs Computer, 2D, 8x8)...")
    new_game_payload = {
        "mode": "hc",
        "dimension": 2,
        "side": 8
    }
    resp = post("/new_game", new_game_payload)
    uuid = resp['uuid']
    print(f"Game Created! UUID: {uuid}")
    
    # 2. Get Initial State
    print("\n2. Fetching Initial State...")
    state = get(f"/game/{uuid}")
    print(f"Current Player: {state['current_player']}")
    print_board(state)
    
    # 3. Valid Moves
    print(f"\n3. Checking Valid Moves (Total: {len(state['valid_moves'])})")
    # Example: Check e2 (1, 4) pawn moves
    # Coordinate format in API handler was string "(1, 4)"
    key = "(1, 4)" 
    if key in state['valid_moves']:
        print(f"Moves for Pawn at {key}: {state['valid_moves'][key]}")
    else:
        print(f"No moves found for {key} (Are we White?)")

    if state['current_player'] != "White":
        print("Expected White to start. Exiting.")
        return

    # 4. Take Turn: e2 -> e4 (1, 4) -> (3, 4)
    print("\n4. Player Move: e2 -> e4 ((1, 4) -> (3, 4))...")
    move_payload = {
        "uuid": uuid,
        "start": [1, 4],
        "end": [3, 4]
    }
    state = post("/take_turn", move_payload)
    print("Move Accepted!")
    print_board(state)
    
    print(f"Current Player: {state['current_player']}")
    
    # 5. Wait for Computer Move
    print("\n5. Waiting for Computer (Black) to move...")
    start_wait = time.time()
    while state['current_player'] == "Black":
        time.sleep(0.5)
        sys.stdout.write(".")
        sys.stdout.flush()
        state = get(f"/game/{uuid}")
        if time.time() - start_wait > 10:
            print("\nTimeout waiting for bot!")
            break
            
    print("\nComputer Moved!")
    print_board(state)
    print(f"Current Player: {state['current_player']}")
    print("\nTest Complete.")

if __name__ == "__main__":
    main()
