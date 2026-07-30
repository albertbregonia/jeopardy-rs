import type { JeopardyPlayer } from "../../../types/Jeopardy";

interface PlayerListProps {
    players: JeopardyPlayer[]
}
export function PlayerList({ players }: PlayerListProps) {
    return (
        <ul className="player-list">
            {players.map(p =>
                <li key={p.name}
                    className="player-card" style={{
                        // shine white when player has buzzed
                        boxShadow: p.buzzed ? "0 0 0.5em #fff" : ""
                    }}>
                    <div className="player-card-name">{p.name}</div>
                    <hr />
                    <div className="player-card-points">{p.points}</div>
                </li>
            )}
        </ul>
    )
}