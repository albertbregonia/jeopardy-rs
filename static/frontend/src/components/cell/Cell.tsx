import type { MouseEventHandler, ReactNode } from 'react'
import './Cell.css'

export interface CellProps {
    children?: ReactNode
    className?: string
    onClick?: MouseEventHandler<HTMLDivElement>
}

// Cells are a simple <div> that takes the size of the available space.
// It is designed to be used as part of a collection of <Cell>'s that will evenly distribute themselves.
export function Cell({ children, className = "cell", onClick = () => { } }: CellProps) {
    return (
        <div className={className} onClick={onClick}>
            {children}
        </div>
    )
}