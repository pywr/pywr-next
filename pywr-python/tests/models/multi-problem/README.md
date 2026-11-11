# Multi-network problem

A multi-network model with two networks, used to test optimisation problems. The supply of
`network1` is limited to 15.0, and the supply of `network2` is the flow delivered by `network1`.

`problem.json` makes the demand of each network variable: first `network2` (between 0.0 and 30.0)
and then `network1` (between 0.0 and 20.0). The objectives are to maximise the mean delivered flow
of each network, and a constraint requires the mean delivered flow of `network2` to be at least 5.0.
