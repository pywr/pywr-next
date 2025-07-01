use std::collections::HashSet;
use std::env;

fn make_builder() -> cc::Build {
    let target = env::var("TARGET").expect("Could not find TARGET in environment.");
    let mut builder = cc::Build::new()
        .cpp(true)
        .warnings(false)
        .extra_warnings(false)
        .define("NDEBUG", None)
        .define("HAVE_STDIO_H", None)
        .define("HAVE_STDLIB_H", None)
        .define("HAVE_STRING_H", None)
        .define("HAVE_INTTYPES_H", None)
        .define("HAVE_STDINT_H", None)
        .define("HAVE_STRINGS_H", None)
        .define("HAVE_SYS_TYPES_H", None)
        .define("HAVE_SYS_STAT_H", None)
        .define("HAVE_UNISTD_H", None)
        .define("HAVE_CMATH", None)
        .define("HAVE_CFLOAT", None)
        // .define("HAVE_DLFCN_H", None)
        .define("HAVE_MEMORY_H", None)
        .to_owned();

    if target.contains("msvc") {
        builder.flag("-EHsc");
        // Flag required for macros __cplusplus to work correctly.
        // See: https://devblogs.microsoft.com/cppblog/msvc-now-correctly-reports-__cplusplus/
        builder.flag("/Zc:__cplusplus");
        builder.flag("/std:c++14");
    } else {
        builder.flag("-std=c++11");
        builder.flag("-w");
    }

    builder
}

const COIN_UTILS_PATH: &str = "vendor/CoinUtils/CoinUtils/src";

const COIN_UTILS_SRCS: [&str; 57] = [
    "CoinAlloc.cpp",
    "CoinBuild.cpp",
    "CoinDenseFactorization.cpp",
    "CoinDenseVector.cpp",
    "CoinError.cpp",
    "CoinFactorization1.cpp",
    "CoinFactorization2.cpp",
    "CoinFactorization3.cpp",
    "CoinFactorization4.cpp",
    "CoinFileIO.cpp",
    "CoinFinite.cpp",
    "CoinIndexedVector.cpp",
    "CoinLpIO.cpp",
    "CoinMessage.cpp",
    "CoinMessageHandler.cpp",
    "CoinModel.cpp",
    "CoinModelUseful2.cpp",
    "CoinModelUseful.cpp",
    "CoinMpsIO.cpp",
    "CoinOslFactorization2.cpp",
    "CoinOslFactorization3.cpp",
    "CoinOslFactorization.cpp",
    "CoinPackedMatrix.cpp",
    "CoinPackedVectorBase.cpp",
    "CoinPackedVector.cpp",
    "CoinParam.cpp",
    "CoinParamUtils.cpp",
    "CoinPostsolveMatrix.cpp",
    "CoinPrePostsolveMatrix.cpp",
    "CoinPresolveDoubleton.cpp",
    "CoinPresolveDual.cpp",
    "CoinPresolveDupcol.cpp",
    "CoinPresolveEmpty.cpp",
    "CoinPresolveFixed.cpp",
    "CoinPresolveForcing.cpp",
    "CoinPresolveHelperFunctions.cpp",
    "CoinPresolveImpliedFree.cpp",
    "CoinPresolveIsolated.cpp",
    "CoinPresolveMatrix.cpp",
    "CoinPresolveMonitor.cpp",
    "CoinPresolvePsdebug.cpp",
    "CoinPresolveSingleton.cpp",
    "CoinPresolveSubst.cpp",
    "CoinPresolveTighten.cpp",
    "CoinPresolveTripleton.cpp",
    "CoinPresolveUseless.cpp",
    "CoinPresolveZeros.cpp",
    "CoinRational.cpp",
    "CoinSearchTree.cpp",
    "CoinShallowPackedVector.cpp",
    "CoinSimpFactorization.cpp",
    "CoinSnapshot.cpp",
    "CoinStructuredModel.cpp",
    "CoinWarmStartBasis.cpp",
    "CoinWarmStartDual.cpp",
    "CoinWarmStartPrimalDual.cpp",
    "CoinWarmStartVector.cpp",
];

/// Compile CoinUtils
fn compile_coin_utils() {
    let mut builder = make_builder();

    builder.flag(format!("-I{COIN_UTILS_PATH}",));

    for src in COIN_UTILS_SRCS.iter() {
        builder.file(format!("{COIN_UTILS_PATH}/{src}",));
    }

    builder.compile("CoinUtils");
}

const OSI_SRC_PATH: &str = "vendor/Osi/Osi/src/Osi";
const OSI_SRCS: [&str; 12] = [
    "OsiAuxInfo.cpp",
    "OsiBranchingObject.cpp",
    "OsiChooseVariable.cpp",
    "OsiColCut.cpp",
    "OsiCut.cpp",
    "OsiCuts.cpp",
    "OsiNames.cpp",
    "OsiPresolve.cpp",
    "OsiRowCut.cpp",
    "OsiRowCutDebugger.cpp",
    "OsiSolverBranch.cpp",
    "OsiSolverInterface.cpp",
];

/// Compiler Osi
///
/// This does not include any of the interfaces, but is required for Cgl.
fn compile_osi() {
    let mut builder = make_builder();

    builder
        .flag(format!("-I{COIN_UTILS_PATH}",))
        .flag(format!("-I{OSI_SRC_PATH}",));

    for src in OSI_SRCS.iter() {
        builder.file(format!("{OSI_SRC_PATH}/{src}",));
    }

    builder.compile("Osi");
}

const CLP_SRC_PATH: &str = "vendor/Clp/Clp/src";
const CLP_OSI_SRC_PATH: &str = "vendor/Clp/Clp/src/OsiClp";

const CLP_SRCS: [&str; 53] = [
    "ClpCholeskyBase.cpp",
    "ClpCholeskyDense.cpp",
    "ClpCholeskyPardiso.cpp",
    "ClpCholeskyTaucs.cpp",
    // Need to have AMD or CHOLMOD to compile ClpCholeskyUfl.
    //"ClpCholeskyUfl.cpp",
    //"ClpCholeskyWssmp.cpp",
    //"ClpCholeskyWssmpKKT.cpp",
    "Clp_C_Interface.cpp",
    "ClpConstraint.cpp",
    "ClpConstraintLinear.cpp",
    "ClpConstraintQuadratic.cpp",
    "ClpDualRowDantzig.cpp",
    "ClpDualRowPivot.cpp",
    "ClpDualRowSteepest.cpp",
    "ClpDummyMatrix.cpp",
    "ClpDynamicExampleMatrix.cpp",
    "ClpDynamicMatrix.cpp",
    "ClpEventHandler.cpp",
    "ClpFactorization.cpp",
    "ClpGubDynamicMatrix.cpp",
    "ClpGubMatrix.cpp",
    "ClpHelperFunctions.cpp",
    "ClpInterior.cpp",
    "ClpLinearObjective.cpp",
    "ClpLsqr.cpp",
    "ClpMatrixBase.cpp",
    "ClpMessage.cpp",
    "ClpModel.cpp",
    "ClpNetworkBasis.cpp",
    "ClpNetworkMatrix.cpp",
    "ClpNode.cpp",
    "ClpObjective.cpp",
    "ClpNonLinearCost.cpp",
    "ClpPackedMatrix.cpp",
    "ClpPdcoBase.cpp",
    "ClpPdco.cpp",
    "ClpPEDualRowDantzig.cpp",
    "ClpPEDualRowSteepest.cpp",
    "ClpPEPrimalColumnDantzig.cpp",
    "ClpPEPrimalColumnSteepest.cpp",
    "ClpPESimplex.cpp",
    "ClpPlusMinusOneMatrix.cpp",
    "ClpPredictorCorrector.cpp",
    "ClpPresolve.cpp",
    "ClpPrimalColumnDantzig.cpp",
    "ClpPrimalColumnPivot.cpp",
    "ClpPrimalColumnSteepest.cpp",
    "ClpQuadraticObjective.cpp",
    "ClpSimplex.cpp",
    "ClpSimplexDual.cpp",
    "ClpSimplexNonlinear.cpp",
    "ClpSimplexOther.cpp",
    "ClpSimplexPrimal.cpp",
    "ClpSolve.cpp",
    "Idiot.cpp",
    "IdiSolve.cpp",
];

fn compile_clp() {
    let mut builder = make_builder();

    builder
        .include(COIN_UTILS_PATH)
        .include(OSI_SRC_PATH)
        .include(CLP_SRC_PATH);

    for src in CLP_SRCS.iter() {
        builder.file(format!("{CLP_SRC_PATH}/{src}",));
    }

    builder.file(format!("{CLP_OSI_SRC_PATH}/OsiClpSolverInterface.cpp",));
    builder.include(CLP_OSI_SRC_PATH);

    builder.compile("Clp");
}

const CGL_SRC_PATH: &str = "vendor/Cgl/Cgl/src";
const CGL_SRCS: [&str; 34] = [
    "CglCutGenerator.cpp",
    "CglMessage.cpp",
    "CglParam.cpp",
    "CglStored.cpp",
    "CglTreeInfo.cpp",
    "CglAllDifferent/CglAllDifferent.cpp",
    "CglClique/CglClique.cpp",
    "CglClique/CglCliqueHelper.cpp",
    "CglDuplicateRow/CglDuplicateRow.cpp",
    "CglFlowCover/CglFlowCover.cpp",
    "CglGMI/CglGMI.cpp",
    "CglGMI/CglGMIParam.cpp",
    "CglGomory/CglGomory.cpp",
    "CglKnapsackCover/CglKnapsackCover.cpp",
    "CglLandP/CglLandP.cpp",
    "CglLandP/CglLandPMessages.cpp",
    "CglLandP/CglLandPSimplex.cpp",
    "CglLandP/CglLandPTabRow.cpp",
    "CglLandP/CglLandPUtils.cpp",
    "CglLandP/CglLandPValidator.cpp",
    "CglMixedIntegerRounding/CglMixedIntegerRounding.cpp",
    "CglMixedIntegerRounding2/CglMixedIntegerRounding2.cpp",
    "CglOddHole/CglOddHole.cpp",
    "CglPreProcess/CglPreProcess.cpp",
    "CglProbing/CglProbing.cpp",
    "CglRedSplit/CglRedSplit.cpp",
    "CglRedSplit/CglRedSplitParam.cpp",
    "CglRedSplit2/CglRedSplit2.cpp",
    "CglRedSplit2/CglRedSplit2Param.cpp",
    "CglResidualCapacity/CglResidualCapacity.cpp",
    "CglSimpleRounding/CglSimpleRounding.cpp",
    "CglTwomir/CglTwomir.cpp",
    "CglZeroHalf/CglZeroHalf.cpp",
    "CglZeroHalf/Cgl012cut.cpp",
];

/// Returns the include directories for Cgl.
fn cgl_include_dirs() -> Vec<String> {
    let mut include_dirs: HashSet<String> = HashSet::new();

    include_dirs.insert(CGL_SRC_PATH.to_string());
    for src in CGL_SRCS.iter() {
        let split: Vec<_> = src.split("/").collect();
        if split.len() == 2 {
            include_dirs.insert(format!("{}/{}", CGL_SRC_PATH, split[0]));
        }
    }

    include_dirs.into_iter().collect()
}

fn compile_cgl() {
    let mut builder = make_builder();

    builder
        .include(COIN_UTILS_PATH)
        .include(OSI_SRC_PATH)
        .include(CLP_SRC_PATH)
        .include(CLP_OSI_SRC_PATH)
        .include(CGL_SRC_PATH);

    for src in CGL_SRCS.iter() {
        builder.file(format!("{CGL_SRC_PATH}/{src}",));
    }

    builder.includes(cgl_include_dirs());
    builder.compile("Cgl");
}

const CBC_SRC_PATH: &str = "vendor/Cbc/Cbc/src";

const CBC_SRCS: [&str; 75] = [
    // Solver
    "Cbc_C_Interface.cpp",
    "CbcCbcParam.cpp",
    "Cbc_ampl.cpp",
    "CbcSolver.cpp",
    "CbcSolverHeuristics.cpp",
    "CbcSolverAnalyze.cpp",
    "CbcSolverExpandKnapsack.cpp",
    "CbcMipStartIO.cpp",
    "CbcLinked.cpp",
    "CbcLinkedUtils.cpp",
    "unitTestClp.cpp",
    // Lib
    "CbcBranchAllDifferent.cpp",
    "CbcBranchCut.cpp",
    "CbcBranchDecision.cpp",
    "CbcBranchDefaultDecision.cpp",
    "CbcBranchDynamic.cpp",
    "CbcBranchingObject.cpp",
    "CbcBranchLotsize.cpp",
    "CbcBranchToFixLots.cpp",
    "CbcCompareDefault.cpp",
    "CbcCompareDepth.cpp",
    "CbcCompareEstimate.cpp",
    "CbcCompareObjective.cpp",
    "CbcConsequence.cpp",
    "CbcClique.cpp",
    "CbcCountRowCut.cpp",
    "CbcCutGenerator.cpp",
    "CbcCutModifier.cpp",
    "CbcCutSubsetModifier.cpp",
    "CbcDummyBranchingObject.cpp",
    "CbcEventHandler.cpp",
    "CbcFathom.cpp",
    "CbcFathomDynamicProgramming.cpp",
    "CbcFixVariable.cpp",
    "CbcFullNodeInfo.cpp",
    "CbcFollowOn.cpp",
    "CbcGeneral.cpp",
    "CbcGeneralDepth.cpp",
    "CbcHeuristic.cpp",
    "CbcHeuristicDINS.cpp",
    "CbcHeuristicDive.cpp",
    "CbcHeuristicDiveCoefficient.cpp",
    "CbcHeuristicDiveFractional.cpp",
    "CbcHeuristicDiveGuided.cpp",
    "CbcHeuristicDiveLineSearch.cpp",
    "CbcHeuristicDivePseudoCost.cpp",
    "CbcHeuristicDiveVectorLength.cpp",
    "CbcHeuristicFPump.cpp",
    "CbcHeuristicGreedy.cpp",
    "CbcHeuristicLocal.cpp",
    "CbcHeuristicPivotAndFix.cpp",
    "CbcHeuristicRandRound.cpp",
    "CbcHeuristicRENS.cpp",
    "CbcHeuristicRINS.cpp",
    "CbcHeuristicVND.cpp",
    "CbcHeuristicDW.cpp",
    "CbcMessage.cpp",
    "CbcModel.cpp",
    "CbcNode.cpp",
    "CbcNodeInfo.cpp",
    "CbcNWay.cpp",
    "CbcObject.cpp",
    "CbcObjectUpdateData.cpp",
    "CbcPartialNodeInfo.cpp",
    "CbcSimpleInteger.cpp",
    "CbcSimpleIntegerDynamicPseudoCost.cpp",
    "CbcSimpleIntegerPseudoCost.cpp",
    "CbcSOS.cpp",
    "CbcStatistics.cpp",
    "CbcStrategy.cpp",
    "CbcSubProblem.cpp",
    "CbcSymmetry.cpp",
    "CbcThread.cpp",
    "CbcTree.cpp",
    "CbcTreeLocal.cpp",
];

fn compile_cbc() {
    let mut builder = make_builder();

    builder
        .include(COIN_UTILS_PATH)
        .include(OSI_SRC_PATH)
        .include(CLP_SRC_PATH)
        .include(CLP_OSI_SRC_PATH)
        .include(CBC_SRC_PATH);

    builder.includes(cgl_include_dirs());

    for src in CBC_SRCS.iter() {
        builder.file(format!("{CBC_SRC_PATH}/{src}",));
    }
    builder.define("CBC_THREAD_SAFE", None);
    builder.define("COIN_HAS_CLP", None);
    builder.compile("Cbc");
}

fn main() {
    compile_cbc();
    compile_cgl();
    compile_osi();
    compile_clp();
    compile_coin_utils();
}
