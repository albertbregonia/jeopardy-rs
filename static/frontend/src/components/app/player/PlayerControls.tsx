import { useState } from 'react'
import './PlayerPanel.css'

interface PlayerControlsProps {
    componentType: "buzzer" | "wager" | "finalJeopardy"
}

export function PlayerControls({ componentType }: PlayerControlsProps) {
    // as each control is only available during a certain game state,
    // this functions as a union of each component and only show
    // the respective input ONLY when it is valid
    const [wager, setWager] = useState(0);
    const [answer, setAnswer] = useState(``);

    function buzzIn() {
        // TODO: api call
        console.log('buzzed');
    }

    function submitWager() {
        // TODO: api call
        console.log(wager);
    }

    function submitFinalJeopardyAnswer() {
        // TODO: api call
        console.log(answer);
    }

    return (
        <div className="player-controls">
            <button
                className="buzzer-button"
                hidden={componentType != "buzzer"}
                onClick={buzzIn}
            >
                BUZZ
            </button>
            <form
                hidden={componentType != "wager"}
                onSubmit={e => {
                    e.preventDefault();
                    submitWager();
                }}
            >
                <input
                    required
                    type="number"
                    placeholder="Final Jeopardy Wager"
                    onChange={e => setWager(e.target.valueAsNumber)}
                />
                <input type="submit" value="Submit" />
            </form>
            <form
                hidden={componentType != "finalJeopardy"}
                onSubmit={e => {
                    e.preventDefault();
                    submitFinalJeopardyAnswer();
                }}
            >
                <input
                    required
                    type="text"
                    placeholder="Final Jeopardy Answer"
                    onChange={e => setAnswer(e.target.value)}
                />
                <input type="submit" value="Submit" />
            </form>
        </div >
    )
}
