import 'package:app_tester_companion/features/proxy_safety/data/proxy_safety_repository.dart';
import 'package:app_tester_companion/features/proxy_safety/presentation/proxy_safety_view_model.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeRepository implements ProxySafetyRepository {
  ProxySafetyStatus current =
      const ProxySafetyStatus(isDeviceOwner: true, isArmed: false);
  String? armedHost;
  int? armedPort;

  @override
  Future<ProxySafetyStatus> arm(
      {required String host, required int port}) async {
    armedHost = host;
    armedPort = port;
    return current = ProxySafetyStatus(
        isDeviceOwner: true, isArmed: true, host: host, port: port);
  }

  @override
  Future<ProxySafetyStatus> disarm() async =>
      current = const ProxySafetyStatus(isDeviceOwner: true, isArmed: false);

  @override
  Future<ProxySafetyStatus> status() async => current;
}

void main() {
  test('arms the configured desktop proxy', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.arm('10.10.10.15', '8080');

    expect(repository.armedHost, '10.10.10.15');
    expect(repository.armedPort, 8080);
    expect(model.status?.isArmed, isTrue);
  });

  test('rejects an invalid proxy port before calling Android', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.arm('10.10.10.15', '0');

    expect(repository.armedPort, isNull);
    expect(model.error, contains('1 to 65535'));
  });
}
