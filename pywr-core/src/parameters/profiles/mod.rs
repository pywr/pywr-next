mod daily;
mod diurnal;
mod monthly;
mod rbf;
mod uniform_drawdown;
mod weekly;

pub use daily::{DailyProfileParameter, DailyProfileParameterBuilder};
pub use diurnal::{DiurnalProfileParameter, DiurnalProfileParameterBuilder};
pub use monthly::{MonthlyInterpDay, MonthlyProfileParameter, MonthlyProfileParameterBuilder};
pub use rbf::{RadialBasisFunction, RbfProfileParameter, RbfProfileParameterBuilder, RbfProfileVariableConfig};
pub use uniform_drawdown::{UniformDrawdownProfileParameter, UniformDrawdownProfileParameterBuilder};
pub use weekly::{
    WeeklyInterpDay, WeeklyProfileError, WeeklyProfileParameter, WeeklyProfileParameterBuilder, WeeklyProfileValues,
};
