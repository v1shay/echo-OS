from __future__ import annotations

from langgraph.graph import END, START, StateGraph

from app.agent.state import EchoGraphState


def build_graph(runtime) -> StateGraph:
    graph = StateGraph(EchoGraphState)
    graph.add_node("prepare_context", runtime.prepare_context)
    graph.add_node("reason", runtime.reason)
    graph.add_node("execute_tools", runtime.execute_tools)
    graph.add_node("verify", runtime.verify)
    graph.add_node("store_outcome", runtime.store_outcome)

    graph.add_edge(START, "prepare_context")
    graph.add_edge("prepare_context", "reason")
    graph.add_conditional_edges(
        "reason",
        runtime.route_after_reason,
        {
            "execute_tools": "execute_tools",
            "verify": "verify",
        },
    )
    graph.add_edge("execute_tools", "reason")
    graph.add_conditional_edges(
        "verify",
        runtime.route_after_verify,
        {
            "reason": "reason",
            "store_outcome": "store_outcome",
        },
    )
    graph.add_edge("store_outcome", END)
    return graph.compile()
