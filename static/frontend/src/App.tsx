import { PlayerPanel } from "./components/app/player/PlayerPanel";
import type { Board, JeopardyPlayer } from "./types/Jeopardy";
import { Login } from "./components/app/login/Login";
import './App.css'

const dummyBoard: Board = {
    categories: Array.from({ length: 6 }, (_, i: number) => ({
        name: `Category ${Math.round(Math.random() * (i + 1) * 10000)} ${Math.round(Math.random() * (i + 1) * 10000)}`,
        questions: Array.from({ length: 6 }, () => ({
            answered: false,
            dailyDouble: false,
            pointValue: Math.round(Math.random() * 1000 + 100),
            question: {
                content: "Question",
                answer: "Answer"
            }
        }))
    }))
};

const dummyPlayers: JeopardyPlayer[] = Array.from({ length: 10 }, (_, i: number) => ({
    points: i * 1000,
    name: `Player ${i}`,
    buzzed: Math.random() < 0.5,
}));

function App() {
    return (
        <>
            <header id="app-header">Jeopardy</header>
            <Login />
            <main id="app-main">
                <PlayerPanel display={{ board: dummyBoard }} players={dummyPlayers} />
            </main>
        </>
    )
}

export default App
