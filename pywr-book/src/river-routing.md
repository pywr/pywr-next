# River routing and attenuation

To account for flow attenuation and travel time in river reaches, Pywr includes a number of routing methods.
Currently available methods are:

- **Delay**: Delay flow by a fixed number of time-steps
- **Muskingum**: Muskingum routing method

## Delay routing

The delay routing method simply delays flow by a fixed number of time-steps. This can be implemented
using either `DelayNode` or `RiverNode` with a routing method of `delay`. Internally, the delay
is implemented using a `DelayParameter` which simply stores the flow values in a queue and returns
the value from the appropriate time-step in the past.

## Muskingum routing

The Muskingum routing method is a widely used hydrological method for simulating the movement of flood waves through
river channels.
It is based on the principle of conservation of mass and momentum, and it uses a simple linear relationship to describe
the storage and flow in a river reach.
The implementation in Pywr is based on the following equation:

\\[ O_t = \left(\frac{\Delta t - 2KX}{2K(1-X) + \Delta t}\right)I_t + \left(\frac{\Delta t + 2KX}{2K(1-X)+\Delta t}\right)I_{t-1} + \left(\frac{2K(1-X)-\Delta t}{2K(1-X)+\Delta t}\right)O_{t-1} \\]

This relates the outflow of the reach at time t \\( (O_t) \\) to the inflow at time t \\( (I_t) \\) and the inflow and
outflow at the previous time step (\\( I_{t-1} \\) and \\( O_{t-1} \\) respectively).
This is implemented in Pywr using a `MuskingumParameter` which uses the above equation to calculate the factors in
an equality constraint of the form:

\\[ O_t - aI_t = b \\]

Where \\( a \\) is the coefficient for the current time-step, \\( b \\) is the sum of the coefficients for the previous
time-step multiplied by their respective values.

### Parameters

The Muskingum routing method requires two parameters:

- **K**: The storage time constant (in time-steps). This represents the time it takes for water to travel through the
  reach.
- **X**: The weighting factor (dimensionless between 0.0 and 0.5). This represents the relative importance of inflow and
  outflow in the reach.

The initial condition can also be specified by the user or set to "steady state". The former
sets the initial inflow and outflow to the specified values, while the latter modifies the constraint to
require that the inflow and outflow are equal at the first time-step.

See also the
HEC-HMS [documentation](https://www.hec.usace.army.mil/confluence/hmsdocs/hmstrm/channel-flow/muskingum-model) on the
Muskingum method for a longer explanation of the parameters and the method.

### Example

The easiest way to use Muskingum routing is to use a `RiverNode` with a routing method of `Muskingum`. This will create
a `MuskingumParameter` internally.
An example of a `RiverNode` with Muskingum routing is shown below:

```json
{
  "meta": {
    "name": "reach1"
  },
  "type": "River",
  "routing_method": {
    "type": "Muskingum",
    "travel_time": {
      "type": "Constant",
      "value": 1.1
    },
    "weight": {
      "type": "Constant",
      "value": 0.25
    },
    "initial_condition": {
      "type": "SteadyState"
    }
  }
}
```