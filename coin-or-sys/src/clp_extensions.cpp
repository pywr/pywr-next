/** This file contains extensions to CLP's C interface to allow setting
the whatsChanged flag and calling the dual method with options. */

#include "ClpSimplex.hpp"

#define CLP_EXTERN_C
#include "Coin_C_defines.h"

extern "C" {

int PywrClp_whatsChanged(const Clp_Simplex *model)
{
    return model->model_->whatsChanged();
}

void PywrClp_setWhatsChanged(Clp_Simplex *model, int value)
{
    model->model_->setWhatsChanged(value);
}

void PywrClp_setUnchangedFlags(Clp_Simplex *model, int flags)
{
    constexpr int PUBLIC_CHANGE_BITS = 0x3ff; // bits 1 through 512

    ClpSimplex *simplex = model->model_;
    int current = simplex->whatsChanged();
    current = (current & ~PUBLIC_CHANGE_BITS)
        | (flags & PUBLIC_CHANGE_BITS);
    simplex->setWhatsChanged(current);
}

int PywrClp_dualWithOptions(
    Clp_Simplex *model,
    int ifValuesPass,
    int startFinishOptions)
{
    return model->model_->dual(ifValuesPass, startFinishOptions);
}

}