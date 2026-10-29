# Simple problem

A simple model with a variable demand, used to test optimisation problems. The supply is limited
to 8.0, so the delivered flow is the smaller of the demand and 8.0.

`problem.json` makes the demand variable (between 0.0 and 20.0), and defines two objectives: to
minimise the mean demand and to maximise the mean delivered flow. A constraint requires the mean
delivered flow to be at least 5.0.
