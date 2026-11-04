The release and publish workflows check out the repository before they run the local ci-green action.
A local action is a file in the workspace, and the gate was starting from an empty one.
