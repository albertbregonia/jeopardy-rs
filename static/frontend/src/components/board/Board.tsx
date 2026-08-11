import type { Board } from "../../jeopardy-rs-sdk/generated/Board"
import { Category } from "../category/Category"
import "./Board.css"

export interface BoardProps {
    board: Board
}

// Categories are a simple <div> that contain a collection of <Categories> 
// that scale to the size of the available space.
export function Board({ board }: BoardProps) {
    return (
        <div className="board">
            {
                board
                    .categories
                    .map(category => <Category key={category.name} category={category} />)
            }
        </div>
    )
}