import { TextCard, type TextCardProps } from "../../textcard/TextCard";
import { Board } from "../../board/Board";
import type { Board as JeopardyBoard, JeopardyPlayer } from "../../../types/Jeopardy";
// import { PlayerControls } from "./PlayerControls";
import "./PlayerPanel.css"
import { PlayerList } from "./PlayerList";

// although this is a union, TypeScript inherits the failures of JavaScript
// and cannot distinguish between interfaces if both props have the same keys.
// the alternative is checking based on inner properties of the types: 
// which creates rigidity in the code and violates the abstraction 
// or use a discriminant and check the type like a string.
// 
// i chose a hybrid approach. this is better as it retains the abstraction but prevents discriminant mismatch
// if it has both, the text card is ignored
interface PlayerPanelProps {
    display: { board: JeopardyBoard } | { textCard: TextCardProps }
    players: JeopardyPlayer[]
}

export function PlayerPanel({ display, players }: PlayerPanelProps) {
    return (
        <div className="player-panel">
            <div className="game-area">
                {
                    ("board" in display)
                        ? <Board board={display.board as JeopardyBoard} />
                        : <TextCard {...(display.textCard as TextCardProps)}></TextCard>
                }
            </div>
            <PlayerList players={players} />
            {/* <PlayerControls /> */}
        </div>
    )
}