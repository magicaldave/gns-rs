window.SIDEBAR_ITEMS = {"struct":[["GnsConnection",""],["GnsConnectionEvent",""],["GnsConnectionInfo",""],["GnsConnectionRealTimeLaneStatus",""],["GnsConnectionRealTimeStatus",""],["GnsError","Wrapper around steam [`EResult`]. The library ensure that the wrapped value is not [`EResult::k_EResultOK`]."],["GnsGlobal","This is an empty type used to wrap the initialization/destruction of the low-level GameNetworkingSockets."],["GnsListenSocket","Opaque wrapper around the low-level [`HSteamListenSocket`]."],["GnsNetworkMessage","Wrapper around the low-level equivalent. This type is used to implements a more type-safe version of messages."],["GnsPollGroup","Opaque wrapper around the low-level [`HSteamNetPollGroup`]."],["GnsSocket","Wrapper around mutiple low-level pointers. The socket is generic over the lifetime of both [`GnsGlobal`] and [`GnsUtils`] as they are required during its lifetime. The generic state `S` is evolving from [`IsCreated`] to either [`IsClient`] or [`IsServer`] depending on the functions used. The drop implementation make sure that everything related to this structure is correctly freed. The user has a strong guarantee that all the embedded points has been validated and thus, the available operations over the socket are safe."],["GnsUtils",""],["IsClient","State of a [`GnsSocket`] that has been determined to be a client, usually via the [`GnsSocket::connect`] call. In this state, the socket hold the data required to receive and send messages."],["IsCreated","Initial state of a [`GnsSocket`]. This state represent a socket that has not been used as a Server or Client implementation. Consequently, the state is empty."],["IsServer","State of a [`GnsSocket`] that has been determined to be a server, usually via the [`GnsSocket::listen`] call. In this state, the socket hold the data required to accept connections and poll them for messages."],["ToReceive",""],["ToSend",""]],"trait":[["GnsDroppable","Simple trait used to allow for a [`GnsSocket`] state to drop itself using the parent structure `socket`."],["IsReady","Common functions available for any [`GnsSocket`] state that is implementing it. Regardless of being a client or server, a ready socket will allow us to query for connection events as well as receive messages."],["MayDrop",""]],"type":[["GnsLane","A lane is represented by a Priority and a Weight"],["GnsLaneId","A lane Id."],["GnsMessageNumber","A network message number. Simple alias for documentation."],["GnsResult","Outcome of many functions from this library, basic type alias with steam [`EResult`] as error. If the result is [`EResult::k_EResultOK`], the value can safely be wrapped, otherwise we return the error."],["Priority","Lane priority"],["Weight","Lane weight"]]};