# Execution platform with the remote ACTION CACHE (and, for re-exec, remote
# EXECUTION) enabled — used by the distributed buck2 CI (gilescope/rebuck2). By
# default (local_enabled=True) execution stays local: buck2 queries the cache by
# action digest, runs misses locally, uploads results. With local_enabled=False
# every action goes through the RE Execution service (rebuck2 workers).
#
# The prelude's own execution_platform hardcodes remote_enabled=False (-> no
# cache). We need remote_enabled=True to activate the ActionCacheChecker path,
# remote_cache_enabled=True for read+write, and remote execution reaches the
# rebuck2 driver's Execution service (misses fall back to local when it advertises
# none, e.g. cache-only rebuck).
#
# Copied from gilescope/rebuck test/platforms/defs.bzl.

def _re_cache_execution_platform_impl(ctx: AnalysisContext) -> list[Provider]:
    constraints = dict()
    constraints.update(ctx.attrs.cpu_configuration[ConfigurationInfo].constraints)
    constraints.update(ctx.attrs.os_configuration[ConfigurationInfo].constraints)
    cfg = ConfigurationInfo(constraints = constraints, values = {})

    name = ctx.label.raw_target()
    platform = ExecutionPlatformInfo(
        label = name,
        configuration = cfg,
        executor_config = CommandExecutorConfig(
            local_enabled = ctx.attrs.local_enabled,
            remote_enabled = True,
            remote_cache_enabled = True,
            remote_execution_use_case = "buck2-default",
            remote_execution_properties = {},
            use_windows_path_separators = ctx.attrs.use_windows_path_separators,
        ),
    )

    return [
        DefaultInfo(),
        platform,
        PlatformInfo(label = str(name), configuration = cfg),
        ExecutionPlatformRegistrationInfo(platforms = [platform]),
    ]

re_cache_execution_platform = rule(
    impl = _re_cache_execution_platform_impl,
    attrs = {
        "cpu_configuration": attrs.dep(providers = [ConfigurationInfo]),
        "os_configuration": attrs.dep(providers = [ConfigurationInfo]),
        # False forces every action through the RE Execution service (rebuck2
        # workers) — no local fallback inside buck2 itself.
        "local_enabled": attrs.bool(default = True),
        "use_windows_path_separators": attrs.bool(default = False),
    },
)
