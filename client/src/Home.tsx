
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { createGame } from "./api";

const Home = () => {
    const navigate = useNavigate();
    const [mode, setMode] = useState("hc");
    const [dim, setDim] = useState(2);
    const [side, setSide] = useState(8);
    const [loading, setLoading] = useState(false);

    const handleCreate = async () => {
        setLoading(true);
        try {
            const resp = await createGame(mode, dim, side);
            navigate(`/game/${resp.uuid}`);
        } catch (e) {
            console.error(e);
            alert("Error creating game");
        } finally {
            setLoading(false);
        }
    };

    return (
        <div className="home-container">
            <h2>New Game</h2>
            <div className="form-group">
                <label>Mode:</label>
                <select value={mode} onChange={e => setMode(e.target.value)}>
                    <option value="hc">Human vs Computer</option>
                    <option value="cc">Computer vs Computer</option>
                    <option value="ch">Computer vs Human</option>
                    <option value="hh">Human vs Human</option>
                </select>
            </div>
            <div className="form-group">
                <label>Dimension:</label>
                <input 
                    type="number" 
                    value={dim} 
                    min={2} 
                    max={4} 
                    onChange={e => setDim(parseInt(e.target.value))} 
                />
            </div>
            <div className="form-group">
                <label>Side Length:</label>
                <input 
                    type="number" 
                    value={side} 
                    min={4} 
                    max={12} 
                    onChange={e => setSide(parseInt(e.target.value))} 
                />
            </div>
            <button className="create-btn" onClick={handleCreate} disabled={loading}>
                {loading ? "Creating..." : "Start Game"}
            </button>
        </div>
    );
};

export default Home;
