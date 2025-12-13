import { BrowserRouter, Route, Routes } from "react-router-dom";
import Home from "./Home";
import Game from "./Game";
import "./App.css";

function App() {
  return (
    <BrowserRouter>
      <div className="app-container">
        <header>
          <h1>HYPERCHESS</h1>
        </header>
        <main>
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/game/:uuid" element={<Game />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}

export default App;
