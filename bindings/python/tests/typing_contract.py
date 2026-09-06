from pathlib import Path

import okc


profile = okc.ProviderProfile(
    name="local",
    kind="ollama",
    endpoint="http://127.0.0.1:11434",
    model="fixture",
)
client = okc.OkcClient([profile], max_concurrent_jobs=2)
project_job: okc.Job[okc.Project] = client.create_project(
    Path("/tmp/types.okc-project"),
    name="Types",
    curator_id="curator",
)
source = okc.SourceInput("fixture", Path("/tmp/vault"))
project_job.cancel()


def exercise_project(value: okc.Project) -> None:
    value.add_source(source)
    value.integrate(
        allow_remote_provider=False,
        remote_disclosure_confirmed=False,
    )
