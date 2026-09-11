param(
    [Parameter(Mandatory)][string]$Repository,
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][string]$Archive
)

$ErrorActionPreference = 'Stop'
# Unlike `release view`, a successful lookup can explicitly report absence.
# Authentication/network failures must not fall through to a write operation.
# GitHub CLI's own shared/fetch.go fetchDraftRelease uses this GraphQL lookup
# for draft visibility; REST's release-by-tag lookup only covers publication.
$owner, $name = $Repository.Split('/', 2)
$query = 'query($owner:String!,$name:String!,$tag:String!){repository(owner:$owner,name:$name){release(tagName:$tag){isDraft}}}'
$json = gh api graphql -f "query=$query" -f "owner=$owner" -f "name=$name" -f "tag=$Tag"
if ($LASTEXITCODE -ne 0) { throw 'Cannot verify beta release state; no archive uploaded.' }
$result = $json | ConvertFrom-Json
if ($result.errors -or $null -eq $result.data.repository) {
    throw 'Cannot verify beta repository; no archive uploaded.'
}
$release = $result.data.repository.release
if ($null -eq $release) {
    gh release create $Tag $Archive --draft --prerelease --generate-notes --verify-tag --repo $Repository
} elseif ($release.isDraft -eq $true) {
    # Replacing a draft asset invalidates its previous acceptance evidence.
    # Do not publish manually while this workflow is running.
    gh release upload $Tag $Archive --clobber --repo $Repository
} else {
    throw 'Refusing to replace an archive in a published beta. Use a new immutable tag.'
}
if ($LASTEXITCODE -ne 0) { throw 'Failed to stage the beta draft archive.' }
