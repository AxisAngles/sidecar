--!strict
type Instance = any
local Instance: any = {}
local game:any = {}
local Enum:any = {}
local warn:any = {}

local HttpService = game:GetService("HttpService")

local instanceToFolderPath: {[Instance]: string} = {}
local folderPathToInstance: {[string]: Instance} = {}
local instanceToFilePath: {[Instance]: string} = {}
local filePathToInstance: {[string]: Instance} = {}
local isServerCreated: {[Instance]: boolean} = {}

local function isScriptLike(instance: Instance): boolean
	return instance:IsA("LuaSourceContainer") -- is this needed?
end

local function getFolderName(instance: Instance): string
	-- Any instance can be treated as a container because it can have children.
	return instance.Name
end

local function getScriptType(instance: Instance): string
	if instance:IsA("ModuleScript") then
		return "module"
	elseif instance:IsA("LocalScript") then
		return "client"
	elseif instance:IsA("Script") then
		if instance.RunContext == Enum.RunContext.Client then
			return "client"
		else
			return "server"
		end
	end

	error(`Unsupported file type for {instance:GetFullName()} ({instance.ClassName})`, 0)
end

local function getFileName(instance: Instance): string
	local scriptType = getScriptType(instance)
	if scriptType == "module" then
		return `{instance.Name}.luau`
	else
		return `{instance.Name}.{scriptType}.luau`
	end
end

local function deduplicateRoots(rootList: {Instance}): {Instance}?
	-- Remove any root that is rootA descendant of another selected root.
	local trueRootList = {}
	for _, rootA in next, rootList do
		local isChild = false
		for _, rootB in next, rootList do
			if rootA:IsDescendantOf(rootB) then
				isChild = true
				break
			end
		end
		if not isChild then
			table.insert(trueRootList, rootA)
		end
	end
	for i, rootA in next, trueRootList do
		for j, rootB in next, trueRootList do
			if i ~= j and rootA.Name == rootB.Name then
				return nil
			end
		end
	end
	return trueRootList
end

local function addRootPath(root: Instance)
	-- Initialize a root-level folder path once.
	local folderName = getFolderName(root)
	instanceToFolderPath[root] = folderName
	folderPathToInstance[folderName] = root
end

-- returns the path from the first cached ancestor to the given instance
-- caches paths to yet uncached instances
local function addFolderPath(instance: Instance?)
	if instance == nil then
		error("instance is not a child of a root")
	elseif instanceToFolderPath[instance] then
		return instanceToFolderPath[instance]
	end

	local parentPath = addFolderPath(instance.Parent)
	local folderName = getFolderName(instance)

	local path = `{parentPath}/{folderName}`
	instanceToFolderPath[instance] = path
	folderPathToInstance[path] = instance

	return path
end

local function addFilePath(instance)
	if instance == nil then
		error("attempt to get path to nil")
	elseif instanceToFilePath[instance] then
		return instanceToFilePath[instance]
	end

	local parentPath = addFolderPath(instance.Parent)
	local fileName = getFileName(instance)

	local path = `{parentPath}/{fileName}`
	instanceToFilePath[instance] = path
	filePathToInstance[path] = instance

	return path
end

local function deconstructFilePath(receivedFilePath: string)
	local filePath = receivedFilePath:gsub("\\", "/")
	local parts = {}
	for token in string.gmatch(filePath, "[^/]+") do
		table.insert(parts, token)
	end
	if #parts == 0 then
		error("empty file path")
	end

	local file = table.remove(parts)::string

	local name, type = string.match(file, "(.-)%.(.-)%.luau?")
	if not type then
		name = string.match(file, "(.-)%.luau?")
		type = "module"
		if not name then
			error(`unsupported file type {file}`)
		end
	elseif type ~= "client" and type ~= "server" then
		error(`unsupported script type, {type}`)
	end

	return {
		parts = parts,
		name = name,
		type = type,
		path = filePath,
	}
end


-- returns an instance and a trunctated path
-- path is nil if instance returned is the file
-- instance is nil if nothing is cached

-- example is folderA/folderB/script.luau
-- if cached["folderA/folderB/script.luau"], return script, -1
-- elseif cached["folderA/folderB"], return folderB, 0
-- elseif cached["folderA"], return folderA, 1
-- else return nil, nil

-- I don't like the name
local function getDeepestCachedInstance(pathData): (Instance?, number?)
	local cached = filePathToInstance[pathData.path]
	if cached then
		return cached, -1
	end

	-- we need to look for chached folders next
	local parts = pathData.parts
	for i = #parts, 1, -1 do
		local folderPath = table.concat(parts, "/", 1, i)
		local folderInstance = folderPathToInstance[folderPath]
		if folderInstance then
			return folderInstance, #parts - i
		end
	end

	return nil, nil
end

-- if we get to this function it is because the instances don't exist
-- or the instances are not cached
-- so we want to find/create the instances
-- and we want to add them to the cache
local function guaranteeFilePathInstances(pathData, instance, pathRemainder): Instance?
	if pathRemainder == -1 then
		return instance
	end

	local parts = pathData.parts
	local pathIndex0 = #parts - pathRemainder

	local currentInstance = instance::Instance
	for i = pathIndex0 + 1, #parts do
		local part = parts[i]
		local foundChild = nil
		local foundCount = 0
		for _, child in next, currentInstance:GetChildren() do
			if child.Name == part then
				foundChild = child
				foundCount += 1
			end
		end
		if foundCount == 0 then
			currentInstance = Instance.new("Folder", currentInstance)
			currentInstance.Name = part
			isServerCreated[currentInstance] = true
		elseif foundCount == 1 then
			currentInstance = foundChild
		else
			return nil -- ambiguous case
		end

		local folderPath = table.concat(parts, "/", 1, i)
		instanceToFolderPath[currentInstance] = folderPath
		folderPathToInstance[folderPath] = currentInstance
	end

	local foundChild = nil
	local foundCount = 0
	for _, child in currentInstance:GetChildren() do
		if pathData.name ~= child.Name then continue end
		if pathData.type ~= getScriptType(child) then continue end
		foundChild = child
		foundCount += 1
	end

	local fileInstance: Instance
	if foundCount == 0 then
		if pathData.type == "module" then
			fileInstance = Instance.new("ModuleScript", currentInstance)
		elseif pathData.type == "server" then
			fileInstance = Instance.new("Script", currentInstance)
		elseif pathData.type == "client" then
			local scriptInstance = Instance.new("Script", currentInstance)
			scriptInstance.RunContext = Enum.RunContext.Client
			fileInstance = scriptInstance
		end
		isServerCreated[fileInstance] = true
	elseif foundCount == 1 then
		fileInstance = foundChild
	else
		return nil -- ambiguous case
	end

	instanceToFilePath[fileInstance] = pathData.path
	filePathToInstance[pathData.path] = fileInstance

	return fileInstance
end

-- returns -1 if the fileInstance doesn't even match
-- return 0 if only the fileInstance matches
local function getPathMatchDepth(fileInstance, pathData)
	if pathData.name ~= fileInstance.Name then return -1 end
	if pathData.type ~= getScriptType(fileInstance) then return -1 end

	local currentInstance = fileInstance.Parent
	local parts = pathData.parts
	for i = #parts, 1, -1 do
		if not currentInstance or parts[i] ~= currentInstance.Name then
			return #parts - i
		end
		currentInstance = currentInstance.Parent
	end

	return #parts
end

-- pick the unique best match by depth; cache the file and up to depth folders
-- returns the matched file instance, or nil if ambiguous / no match
local function synchronizeInstancesWithPath(pathData)
	local bestInstance = nil
	local bestDepth = -1
	local bestCount = 0
	for _, fileInstance in next, game:GetDescendants() do
		local matchScore = getPathMatchDepth(fileInstance, pathData)
		if matchScore > bestDepth then
			bestInstance = fileInstance
			bestDepth = matchScore
			bestCount = 1
		elseif matchScore == bestDepth then
			bestCount += 1
		end
	end

	if not bestInstance or bestCount > 1 then
		return nil
	end

	-- cache the file
	filePathToInstance[pathData.path] = bestInstance
	instanceToFilePath[bestInstance] = pathData.path

	-- cache *up to* the matched depth worth of ancestor folders (suffix of parts)
	-- parts = {"A","B","C"}; if depth=2, we cache "A/B/C" parent for j=3 and "A/B" parent for j=2
	local folderInstance = bestInstance.Parent
	local parts = pathData.parts
	for j = #parts, #parts - bestDepth + 1, -1 do
		local folderPath = table.concat(parts, "/", 1, j)
		instanceToFolderPath[folderInstance] = folderPath
		folderPathToInstance[folderPath] = folderInstance
		folderInstance = folderInstance.Parent
	end

	return bestInstance
end







local function updateScript(receivedBody)
	local receivedFilePath, source = string.match(receivedBody, "(.-)\n(.*)")
	if not source then
		warn(`[Sidecar] receivedBody contains no source`)
		return
	end
	if not receivedFilePath then
		warn(`[Sidecar] receivedBody contains no filePath`)
		return
	end

	local pathData = deconstructFilePath(receivedFilePath)

	local instance, pathRemainder = getDeepestCachedInstance(pathData)

	local fileInstance
	if instance and pathRemainder then
		fileInstance = guaranteeFilePathInstances(pathData, instance, pathRemainder)::any
	else
		fileInstance = synchronizeInstancesWithPath(pathData)
	end

	if not fileInstance then
		warn("cannot unambiguously create/locate script with path", pathData.path)
		return
	end

	fileInstance.Source = source
end

local function deleteFile(receivedFilePath)
	local pathData = deconstructFilePath(receivedFilePath)
	local fileInstance = filePathToInstance[pathData.path]
	if not fileInstance then return end
	if not isServerCreated[fileInstance] then return end
	instanceToFilePath[fileInstance] = nil
	filePathToInstance[pathData.path] = nil

	local folderInstance = fileInstance
	while isServerCreated[folderInstance] do
		if #folderInstance:GetChildren() ~= 0 then return end
		local folderPath = instanceToFolderPath[folderInstance]
		if folderPath then
			instanceToFolderPath[folderInstance] = nil
			folderPathToInstance[folderPath] = nil
		end
		folderInstance = (folderInstance::any).Parent
	end
end












-- written by chatGPT
-- implementation: export
local function exportSelected()
	-- 1) Gather and dedupe roots
	local roots = deduplicateRoots(game.Selection:Get())
	if not roots then
		warn("[Sidecar] Roots are not uniquely named.")
		return
	elseif #roots == 0 then
		warn("[Sidecar] No roots selected.")
		return
	end

	-- 2) Initialize root paths
	for _, root in next, roots do
		addRootPath(root)
	end

	-- 3) Discover all scripts under each root and POST write_file
	local exported = 0
	for _, root in next, roots do
		local childrenInclusive = root:GetDescendants()
		table.insert(childrenInclusive, root)
		for _, instance in next, childrenInclusive do
			if not isScriptLike(instance) then continue end

			local path = addFilePath(instance)
			local src = (instance :: any).Source
			-- Body format: "<path>\n<source>"
			--local payload = `{path}\n{src}`
			--local ok = http_ok("POST", SIDECAR_HOST .. "/write_file", payload, "TextPlain")
			local success, message = pcall(HttpService.PostAsync, HttpService,
				"127.0.0.1:8080/write_file",
				`{path}\n{src}`)
			if success then
				exported += 1
			else
				warn(`[Sidecar] Could not export {path}, got error {message}`)
			end
		end
	end

	print(`[Sidecar] Export complete. Files: {exported}`)
end

-- implementation: clear
local function clearSidecar()
	--http_ok("POST", SIDECAR_HOST .. "/clear", "", "ApplicationJson")

	local success, message = pcall(HttpService.PostAsync, HttpService,
		"127.0.0.1:8080/clear", "")
	if not success then
		warn(`[Sidecar] Could not clear, got error {message}`)
		return
	end
	-- Clear caches since the filesystem view is now empty
	table.clear(instanceToFilePath)
	table.clear(filePathToInstance)
	table.clear(instanceToFolderPath)
	table.clear(folderPathToInstance)
	print("[Sidecar] Cleared sidecar workspace and caches.")
end
