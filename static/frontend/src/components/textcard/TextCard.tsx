import type { ReactNode } from "react"
import "./TextCard.css"

export interface TextCardProps {
    title: string,
    children?: ReactNode,
}

export function TextCard({ title, children }: TextCardProps) {
    return (
        <div className="textcard">
            <h1 className="textcard-title">{title}</h1>
            <hr />
            <div className="textcard-textcontent">{children}</div>
        </div>
    )
}