import { useState } from "react";
import { TextCard } from "../../textcard/TextCard";
import { Board } from "../../board/Board";
import type { Board as JeopardyBoard } from "../../../types/Jeopardy";
import "./PlayerPanel.css"

const jeopardy: JeopardyBoard = {
    categories: [
        {
            name: "Lorem ipsum dolor sit amet consectetur adipisicing elit. Vero tempore illo eligendi maxime aliquam incidunt sed ut veniam, ratione sapiente! Modi deserunt qui provident ratione, voluptas quia dolorem veniam minima!",
            questions: Array.from({ length: 2 }, () => ({
                answered: false,
                dailyDouble: false,
                pointValue: Math.round(Math.random() * 1000 + 100),
                question: {
                    content: "Question",
                    answer: "Answer"
                }
            }))
        },
        {
            name: "Lorem ipsum dolor sit amet consectetur adipisicing elit. Corporis fugiat ex fuga cupiditate magni id distinctio possimus iusto tempora, maxime accusantium exercitationem labore. Blanditiis id quisquam debitis, doloremque deleniti aliquam.",
            questions: Array.from({ length: 5 }, () => ({
                answered: false,
                dailyDouble: false,
                pointValue: Math.round(Math.random() * 1000 + 100),
                question: {
                    content: "Question",
                    answer: "Answer"
                }
            }))
        },
        ...Array.from({ length: 10 }, (_, i: number) => ({
            name: `Category ${i}`,
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
    ]
}

export interface PlayerPanelProps {

}

export function PlayerPanel({ }: PlayerPanelProps) {
    const [showBoard, setShowBoard] = useState(true);
    const [b, setBoard] = useState(jeopardy);
    const longText = "Lorem ipsum dolor sit amet, consectetur adipisicing elit. Consectetur temporibus pariatur laboriosam blanditiis totam voluptas debitis nulla dolorem, exercitationem aliquam doloribus ad non, maxime amet! Accusamus iusto delectus nesciunt. Excepturi.";
    return (
        <>
            <div id="game-area">
                {
                    showBoard ? <Board board={b} />
                        : <TextCard title="NAMES OF PEOPLE">
                            <div>{longText}</div>
                        </TextCard>
                }
            </div>
            <div id="player-controls">
                <button onClick={() => setShowBoard(!showBoard)}>Toggle</button>
                <button onClick={() => setBoard(j => randomValue(j))}>Re-render</button>
            </div>
        </>
    )
}

// sample test to show how all the objects will re-render when the collection gets updated
function randomValue(jeopardy: JeopardyBoard): JeopardyBoard {
    let clone: JeopardyBoard = JSON.parse(JSON.stringify(jeopardy));
    clone
        .categories
        .forEach(c =>
            c.questions
                .forEach(q => { // we use some extremely large value to test overflow 
                    q.pointValue = Math.round(Math.random() * 10_000_000 + 1_000_000);
                })
        );
    return clone;
}